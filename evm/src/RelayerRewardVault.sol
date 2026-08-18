// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

/// @dev Subconjunto do Mailbox v3 do Hyperlane usado como prova de entrega
///      (Mailbox.sol: `deliveries[id] = Delivery(msg.sender, uint48(block.number))`).
interface IMailboxDelivery {
    function processor(bytes32 _id) external view returns (address);

    function processedAt(bytes32 _id) external view returns (uint48);
}

/**
 * @title RelayerRewardVault
 * @notice Vault beneficiary do IGP (BSC/Ethereum) — spec §07. Um relayer não
 *         recebe por estar numa lista: recebe porque `mailbox.processor(id)`
 *         diz que foi ELE quem processou a mensagem.
 *
 *         O `claim()` do IGP upstream é permissionless e empurra o saldo para o
 *         beneficiary — este contrato — portanto NÃO há Sweep: basta o
 *         `receive()` payable. O relayer pode chamar `igp.claim()` e
 *         `vault.claim(ids)` na mesma transação (multicall próprio).
 *
 *         Owner = multisig dos validadores (com signatários externos, spec §04).
 */
contract RelayerRewardVault {
    // ============ Errors ============
    error NotOwner();
    error NotPendingOwner();
    error VaultPaused();
    error EmptyBatch();
    error NotDelivered(bytes32 id);
    error NotProcessor(bytes32 id, address processor);
    error ClaimWindowExpired(bytes32 id, uint256 deadline, uint256 current);
    error AlreadyClaimed(bytes32 id, address claimant);
    error InsufficientPool(uint256 needed, uint256 available);
    error ZeroReward();
    error ZeroWindow();
    error ZeroAddress();
    error TransferFailed();
    error Reentrancy();

    // ============ Events ============
    event RewardClaimed(bytes32 indexed id, address indexed claimant, uint256 amount);
    event BatchClaimed(address indexed claimant, uint256 count, uint256 total);
    event Funded(address indexed from, uint256 amount);
    event ParamsUpdated(uint256 rewardPerDelivery, uint256 claimWindowBlocks);
    event PauseSet(bool paused);
    event SurplusWithdrawn(address indexed to, uint256 amount);
    event OwnershipTransferStarted(address indexed current, address indexed pending);
    event OwnershipTransferred(address indexed previous, address indexed current);

    // ============ Storage ============
    IMailboxDelivery public immutable mailbox;

    address public owner;
    address public pendingOwner;

    /// tarifa fixa por entrega comprovada, em wei da moeda nativa
    uint256 public rewardPerDelivery;
    /// janela de resgate em blocos, contada do bloco da entrega
    uint256 public claimWindowBlocks;
    bool public paused;

    /// message id → quem resgatou (endereço zero = ainda não resgatado)
    mapping(bytes32 id => address claimant) public claimedBy;

    uint256 public totalPaid;
    uint256 public totalClaims;

    uint256 private _entered; // reentrancy guard (1 = livre, 2 = ocupado)

    // ============ Modifiers ============
    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier nonReentrant() {
        if (_entered == 2) revert Reentrancy();
        _entered = 2;
        _;
        _entered = 1;
    }

    // ============ Constructor ============
    constructor(
        address _mailbox,
        address _owner,
        uint256 _rewardPerDelivery,
        uint256 _claimWindowBlocks
    ) {
        if (_mailbox == address(0) || _owner == address(0)) revert ZeroAddress();
        if (_rewardPerDelivery == 0) revert ZeroReward();
        if (_claimWindowBlocks == 0) revert ZeroWindow();
        mailbox = IMailboxDelivery(_mailbox);
        owner = _owner;
        rewardPerDelivery = _rewardPerDelivery;
        claimWindowBlocks = _claimWindowBlocks;
        _entered = 1;
        emit OwnershipTransferred(address(0), _owner);
    }

    /// @notice Obrigatório: é por aqui que o `claim()` do IGP deposita a arrecadação.
    receive() external payable {
        emit Funded(msg.sender, msg.value);
    }

    // ============ Relayer ============

    /**
     * @notice Resgata a recompensa das entregas comprovadas. ATÔMICO: qualquer id
     *         inválido reverte o lote inteiro (duplicata no lote cai em
     *         AlreadyClaimed, pois o registro é gravado dentro do loop).
     */
    function claim(bytes32[] calldata ids) external nonReentrant {
        if (paused) revert VaultPaused();
        uint256 count = ids.length;
        if (count == 0) revert EmptyBatch();

        uint256 reward = rewardPerDelivery;
        uint256 window = claimWindowBlocks;

        for (uint256 i = 0; i < count; ++i) {
            bytes32 id = ids[i];

            address previous = claimedBy[id];
            if (previous != address(0)) revert AlreadyClaimed(id, previous);

            address processor = mailbox.processor(id);
            if (processor == address(0)) revert NotDelivered(id);
            if (processor != msg.sender) revert NotProcessor(id, processor);

            uint256 deadline = uint256(mailbox.processedAt(id)) + window;
            if (block.number > deadline) {
                revert ClaimWindowExpired(id, deadline, block.number);
            }

            claimedBy[id] = msg.sender; // effects-first
            emit RewardClaimed(id, msg.sender, reward);
        }

        uint256 total = reward * count;
        if (address(this).balance < total) {
            revert InsufficientPool(total, address(this).balance);
        }

        totalPaid += total;
        totalClaims += count;
        emit BatchClaimed(msg.sender, count, total);

        (bool ok, ) = msg.sender.call{value: total}("");
        if (!ok) revert TransferFailed();
    }

    // ============ Views ============

    /// @notice Quantas entregas o pool atual consegue pagar.
    function claimsPayable() external view returns (uint256) {
        return address(this).balance / rewardPerDelivery;
    }

    // ============ Owner (multisig) ============

    function setParams(uint256 _rewardPerDelivery, uint256 _claimWindowBlocks) external onlyOwner {
        if (_rewardPerDelivery == 0) revert ZeroReward();
        if (_claimWindowBlocks == 0) revert ZeroWindow();
        rewardPerDelivery = _rewardPerDelivery;
        claimWindowBlocks = _claimWindowBlocks;
        emit ParamsUpdated(_rewardPerDelivery, _claimWindowBlocks);
    }

    function setPause(bool _paused) external onlyOwner {
        paused = _paused;
        emit PauseSet(_paused);
    }

    function withdrawSurplus(address to, uint256 amount) external onlyOwner {
        if (to == address(0)) revert ZeroAddress();
        emit SurplusWithdrawn(to, amount);
        (bool ok, ) = to.call{value: amount}("");
        if (!ok) revert TransferFailed();
    }

    /// posse em dois passos — multisig novo precisa aceitar (evita transferir p/ endereço morto)
    function transferOwnership(address _pending) external onlyOwner {
        if (_pending == address(0)) revert ZeroAddress();
        pendingOwner = _pending;
        emit OwnershipTransferStarted(owner, _pending);
    }

    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotPendingOwner();
        emit OwnershipTransferred(owner, msg.sender);
        owner = msg.sender;
        pendingOwner = address(0);
    }
}
