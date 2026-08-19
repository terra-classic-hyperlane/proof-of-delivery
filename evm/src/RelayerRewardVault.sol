// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

/// @dev Subconjunto do Mailbox v3 do Hyperlane usado como prova de entrega
///      (Mailbox.sol: `deliveries[id] = Delivery(msg.sender, uint48(block.number))`).
interface IMailboxDelivery {
    function processor(bytes32 _id) external view returns (address);

    function processedAt(bytes32 _id) external view returns (uint48);

    /// Despacha uma mensagem pela ponte (recibo de volta). Paga o hook com msg.value.
    function dispatch(uint32 destination, bytes32 recipient, bytes calldata body)
        external
        payable
        returns (bytes32 messageId);

    function quoteDispatch(uint32 destination, bytes32 recipient, bytes calldata body)
        external
        view
        returns (uint256);
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
    // ---- v2 ClaimRemote ----
    error NotAttestor();
    error NoBinding(address operator, uint32 domain);
    error NoRemoteReward(uint32 domain);
    error RemoteAlreadyClaimed(bytes32 id, address executor);
    error AlreadyAttested(bytes32 id, address attestor);
    error BadRemoteQuorum();
    error NotMailbox();
    error UntrustedRouter(uint32 origin, bytes32 sender);
    error MixedOrigin();
    error UnknownExecutor(bytes32 id, address executor);
    error MalformedReceipt();
    error NoRouter(uint32 domain);

    // ============ Events ============
    event RewardClaimed(bytes32 indexed id, address indexed claimant, uint256 amount);
    event BatchClaimed(address indexed claimant, uint256 count, uint256 total);
    event Funded(address indexed from, uint256 amount);
    event ParamsUpdated(uint256 rewardPerDelivery, uint256 claimWindowBlocks);
    event PauseSet(bool paused);
    event SurplusWithdrawn(address indexed to, uint256 amount);
    event OwnershipTransferStarted(address indexed current, address indexed pending);
    event OwnershipTransferred(address indexed previous, address indexed current);
    // ---- v2 ClaimRemote ----
    event RemoteConfigSet(uint256 attestorCount, uint256 quorum);
    event RemoteBindingSet(address indexed operator, uint32 indexed domain, string remoteAddress);
    event RemoteRewardSet(uint32 indexed domain, uint256 reward);
    event RemoteAttested(bytes32 indexed id, address indexed attestor, address indexed executor);
    event RemotePaid(bytes32 indexed id, address indexed executor, uint32 domain, uint256 amount);
    event OperatorAddressSet(uint32 indexed index, uint32 indexed domain, string addr);
    event RemoteRouterSet(uint32 indexed domain, bytes32 router);
    event IsmSet(address ism);
    event ReceiptSent(uint32 indexed originDomain, uint256 count, bytes32 messageId);
    event ReceiptPaid(bytes32 indexed id, uint32 indexed operatorIndex, address recipient, uint256 amount);

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

    // ---- v2 ClaimRemote: taxa de ORIGEM paga por entrega remota atestada.
    //      Esta chain não enxerga as outras; a confiança fica no conjunto de
    //      atestadores + vínculos + quórum (mesmo modelo do vault da Solana),
    //      com dano limitado: 1 pagamento por id, recompensa fixa por domínio.
    struct RemoteClaimRecord {
        address executor;
        uint32 domain;
        uint256 amount;
        uint256 blockNumber;
    }

    address[] public remoteAttestors;
    mapping(address attestor => bool) public isRemoteAttestor;
    uint256 public remoteQuorum;
    /// operador local → domínio remoto → endereço do executor lá (ex.: terra1…)
    mapping(address operator => mapping(uint32 domain => string)) public remoteBinding;
    /// recompensa fixa por entrega remota, por domínio (0 = desativado)
    mapping(uint32 domain => uint256) public remoteReward;
    /// message id → pagamento remoto efetuado (executor != 0 = pago; anti-duplo)
    mapping(bytes32 id => RemoteClaimRecord) public remoteClaimed;
    /// id → atestador → executor apontado (anti re-atestação)
    mapping(bytes32 id => mapping(address attestor => address executor)) public remoteVote;
    /// id → executor → nº de atestações concordantes
    mapping(bytes32 id => mapping(address executor => uint256)) public remoteVoteCount;
    uint256 public totalRemotePaid;

    // ---- Fase 1 (recibo trustless): registro de/para global + routers ----
    /// domínio DESTE vault (imutável, setado no constructor)
    uint32 public immutable localDomain;
    /// índice do operador → domínio → endereço naquele domínio (string, multi-VM)
    mapping(uint32 index => mapping(uint32 domain => string)) public operatorAddress;
    /// reverse-lookup: executor LOCAL → índice do operador (+1; 0 = ausente)
    mapping(address local => uint256 indexPlus1) internal _operatorOfLocalPlus1;
    /// router confiável (nosso vault) por domínio, em bytes32 (convenção Hyperlane)
    mapping(uint32 domain => bytes32 router) public remoteRouter;
    uint32 public operatorCount;
    /// ISM que valida os recibos recebidos (o mesmo do warp da rota). 0 = default
    /// do Mailbox. Necessário quando o ISM default não conhece a origem do recibo.
    address public ism;

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
        uint256 _claimWindowBlocks,
        uint32 _localDomain
    ) {
        if (_mailbox == address(0) || _owner == address(0)) revert ZeroAddress();
        if (_rewardPerDelivery == 0) revert ZeroReward();
        if (_claimWindowBlocks == 0) revert ZeroWindow();
        mailbox = IMailboxDelivery(_mailbox);
        localDomain = _localDomain;
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

    // ============ v2 — ClaimRemote ============

    /// @notice Owner: define atestadores e quórum de atestações CONCORDANTES.
    function setRemoteOperators(address[] calldata attestors_, uint256 quorum_) external onlyOwner {
        if (quorum_ == 0 || quorum_ > attestors_.length) revert BadRemoteQuorum();
        for (uint256 i = 0; i < remoteAttestors.length; ++i) {
            isRemoteAttestor[remoteAttestors[i]] = false;
        }
        delete remoteAttestors;
        for (uint256 i = 0; i < attestors_.length; ++i) {
            if (attestors_[i] == address(0)) revert ZeroAddress();
            isRemoteAttestor[attestors_[i]] = true;
            remoteAttestors.push(attestors_[i]);
        }
        remoteQuorum = quorum_;
        emit RemoteConfigSet(attestors_.length, quorum_);
    }

    /// @notice Owner: vincula o endereço REMOTO do operador num domínio ("" remove).
    function setRemoteBinding(address operator, uint32 domain, string calldata remoteAddress)
        external
        onlyOwner
    {
        remoteBinding[operator][domain] = remoteAddress;
        emit RemoteBindingSet(operator, domain, remoteAddress);
    }

    /// @notice Owner: recompensa fixa por entrega remota no domínio (0 desativa).
    function setRemoteReward(uint32 domain, uint256 reward) external onlyOwner {
        remoteReward[domain] = reward;
        emit RemoteRewardSet(domain, reward);
    }

    /**
     * @notice Atestador: afirma que as mensagens (despachadas DESTE mailbox p/
     *         `domain` — o message id é o MESMO nas duas chains) foram entregues
     *         lá pelo endereço vinculado ao `executor` (address(0) = o próprio
     *         atestador). Ao atingir o quórum de atestações CONCORDANTES paga a
     *         recompensa — UMA vez por id. ATÔMICO: id inválido reverte o lote.
     */
    function attestRemoteDelivery(uint32 domain, bytes32[] calldata ids, address executor)
        external
        nonReentrant
    {
        if (paused) revert VaultPaused();
        if (ids.length == 0) revert EmptyBatch();
        if (!isRemoteAttestor[msg.sender]) revert NotAttestor();
        address exec = executor == address(0) ? msg.sender : executor;
        if (bytes(remoteBinding[exec][domain]).length == 0) revert NoBinding(exec, domain);
        uint256 reward = remoteReward[domain];
        if (reward == 0) revert NoRemoteReward(domain);

        uint256 payCount = 0;
        for (uint256 i = 0; i < ids.length; ++i) {
            bytes32 id = ids[i];
            address paidTo = remoteClaimed[id].executor;
            if (paidTo != address(0)) revert RemoteAlreadyClaimed(id, paidTo);
            if (remoteVote[id][msg.sender] != address(0)) revert AlreadyAttested(id, msg.sender);
            remoteVote[id][msg.sender] = exec;
            // ANTI-AUTOPAGAMENTO: com quórum >= 2, o voto do PRÓPRIO beneficiário
            // NÃO conta — o pagamento exige `quorum` atestadores INDEPENDENTES.
            // (registra o voto para impedir re-voto, mas não avança o quórum.)
            uint256 agree = remoteVoteCount[id][exec];
            if (!(remoteQuorum >= 2 && msg.sender == exec)) {
                agree = ++remoteVoteCount[id][exec];
            }
            emit RemoteAttested(id, msg.sender, exec);
            if (agree >= remoteQuorum) {
                // effects-first: marca pago antes da transferência
                remoteClaimed[id] = RemoteClaimRecord(exec, domain, reward, block.number);
                ++payCount;
                emit RemotePaid(id, exec, domain, reward);
            }
        }
        if (payCount > 0) {
            uint256 total = reward * payCount;
            if (address(this).balance < total) revert InsufficientPool(total, address(this).balance);
            totalRemotePaid += total;
            (bool ok, ) = exec.call{value: total}("");
            if (!ok) revert TransferFailed();
        }
    }

    function remoteAttestorCount() external view returns (uint256) {
        return remoteAttestors.length;
    }

    // ---- Fase 1: registro de/para global + routers (só owner) ----

    /// @notice Grava o endereço do operador `index` no `domain` ("" remove). Se
    ///         `domain` == localDomain, mantém o reverse-lookup (executor→índice).
    function setOperatorAddress(uint32 index, uint32 domain, string calldata addr) external onlyOwner {
        if (domain == localDomain) {
            string memory old = operatorAddress[index][domain];
            if (bytes(old).length != 0) {
                _operatorOfLocalPlus1[_parseAddr(old)] = 0;
            }
            if (bytes(addr).length != 0) {
                _operatorOfLocalPlus1[_parseAddr(addr)] = uint256(index) + 1;
            }
        }
        operatorAddress[index][domain] = addr;
        if (bytes(addr).length != 0 && index + 1 > operatorCount) operatorCount = index + 1;
        emit OperatorAddressSet(index, domain, addr);
    }

    /// @notice Índice do operador dono de um executor LOCAL (0 = não registrado).
    function operatorOfLocal(address local) external view returns (bool found, uint32 index) {
        uint256 p = _operatorOfLocalPlus1[local];
        return (p != 0, uint32(p == 0 ? 0 : p - 1));
    }

    /// @notice Router (nosso vault) confiável de um domínio (bytes32(0) remove).
    function setRemoteRouter(uint32 domain, bytes32 router) external onlyOwner {
        remoteRouter[domain] = router;
        emit RemoteRouterSet(domain, router);
    }

    /// @notice Owner: ISM que valida os recibos (aponte para o ISM do warp da
    ///         rota). 0 = usa o ISM default do Mailbox.
    function setIsm(address _ism) external onlyOwner {
        ism = _ism;
        emit IsmSet(_ism);
    }

    /// @dev converte a string "0x…40hex" no address (para o reverse-lookup local).
    function _parseAddr(string memory s) internal pure returns (address a) {
        bytes memory b = bytes(s);
        require(b.length == 42 && b[0] == "0" && (b[1] == "x" || b[1] == "X"), "addr");
        uint160 r;
        for (uint256 i = 2; i < 42; ++i) {
            uint8 c = uint8(b[i]);
            uint8 v = c >= 48 && c <= 57 ? c - 48 : (c >= 97 && c <= 102 ? c - 87 : c - 55);
            r = r * 16 + v;
        }
        a = address(r);
    }

    /// @notice Quanto estes ids PAGARIAM se confirmados (ainda não pagos) — para
    ///         o operador decidir se vale o gás de enviar o recibo/atestação.
    ///         `amount` = payableCount × recompensa do domínio.
    function quoteRemote(uint32 domain, bytes32[] calldata ids)
        external
        view
        returns (uint256 amount, uint256 payableCount)
    {
        uint256 r = remoteReward[domain];
        for (uint256 i = 0; i < ids.length; ++i) {
            if (remoteClaimed[ids[i]].executor == address(0)) {
                payableCount += 1;
                amount += r;
            }
        }
    }

    // ============ Recibo trustless (Fase 2/3) ============

    /// @notice PAPEL DESTINO. Prova que estas MENSAGENS (bytes completos) foram
    ///         entregues AQUI e despacha UM recibo de volta ao vault de origem.
    ///         `id = keccak256(message)` e o domínio de origem é LIDO da mensagem
    ///         (bytes [1..5]) — o operador não consegue forjar o destino do recibo.
    ///         Operador paga o gás do recibo via msg.value. Todas as msgs do lote
    ///         devem ter a MESMA origem (um recibo → um vault de origem).
    function sendReceipt(bytes[] calldata messages) external payable nonReentrant returns (bytes32) {
        if (paused) revert VaultPaused();
        uint256 n = messages.length;
        if (n == 0) revert EmptyBatch();

        uint32 originDomain = _originOf(messages[0]);
        bytes memory body = new bytes(n * 36); // id(32) + operatorIndex(4) por entrega
        for (uint256 i = 0; i < n; ++i) {
            if (_originOf(messages[i]) != originDomain) revert MixedOrigin();
            bytes32 id = keccak256(messages[i]);
            address exec = mailbox.processor(id);
            if (exec == address(0)) revert NotDelivered(id);
            (bool found, uint32 idx) = this.operatorOfLocal(exec);
            if (!found) revert UnknownExecutor(id, exec);
            // grava id + idx no corpo (big-endian)
            uint256 off = i * 36;
            for (uint256 b = 0; b < 32; ++b) body[off + b] = id[b];
            body[off + 32] = bytes1(uint8(idx >> 24));
            body[off + 33] = bytes1(uint8(idx >> 16));
            body[off + 34] = bytes1(uint8(idx >> 8));
            body[off + 35] = bytes1(uint8(idx));
        }
        bytes32 router = remoteRouter[originDomain];
        if (router == bytes32(0)) revert NoRouter(originDomain);
        bytes32 mid = mailbox.dispatch{value: msg.value}(originDomain, router, body);
        emit ReceiptSent(originDomain, n, mid);
        return mid;
    }

    /// @notice PAPEL ORIGEM. Recebe o recibo do Mailbox. Só aceita do próprio
    ///         Mailbox e de um `sender` == router registrado do domínio de origem.
    ///         Paga cada id (não pago) ao endereço do operador N no NOSSO registro
    ///         (localDomain). Idempotente: id já pago é ignorado, não reverte.
    function handle(uint32 origin, bytes32 sender, bytes calldata body) external payable {
        if (msg.sender != address(mailbox)) revert NotMailbox();
        if (remoteRouter[origin] == bytes32(0) || sender != remoteRouter[origin]) {
            revert UntrustedRouter(origin, sender);
        }
        if (body.length == 0 || body.length % 36 != 0) revert MalformedReceipt();
        uint256 reward = remoteReward[origin]; // origin = onde a entrega ocorreu
        uint256 count = body.length / 36;
        for (uint256 i = 0; i < count; ++i) {
            uint256 off = i * 36;
            bytes32 id;
            assembly { id := calldataload(add(body.offset, off)) }
            if (remoteClaimed[id].executor != address(0)) continue; // idempotente
            uint32 idx = (uint32(uint8(body[off + 32])) << 24)
                | (uint32(uint8(body[off + 33])) << 16)
                | (uint32(uint8(body[off + 34])) << 8)
                | uint32(uint8(body[off + 35]));
            string memory payoutStr = operatorAddress[idx][localDomain];
            if (bytes(payoutStr).length == 0 || reward == 0) continue; // sem registro/recompensa
            address payout = _parseAddr(payoutStr);
            remoteClaimed[id] = RemoteClaimRecord(payout, origin, reward, block.number);
            if (address(this).balance >= reward) {
                totalRemotePaid += reward;
                (bool ok, ) = payout.call{value: reward}("");
                if (!ok) revert TransferFailed();
                emit ReceiptPaid(id, idx, payout, reward);
            } else {
                emit ReceiptPaid(id, idx, payout, 0); // registrado; pool sem fundo (semear)
            }
        }
    }

    /// @notice ISM do recipient: o configurado (do warp da rota) ou 0 = default.
    function interchainSecurityModule() external view returns (address) {
        return ism;
    }

    /// @dev domínio de origem da msg Hyperlane: version(1)+nonce(4) → origin em [5..9].
    function _originOf(bytes calldata message) internal pure returns (uint32) {
        require(message.length >= 9, "msg");
        return (uint32(uint8(message[5])) << 24) | (uint32(uint8(message[6])) << 16)
            | (uint32(uint8(message[7])) << 8) | uint32(uint8(message[8]));
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
