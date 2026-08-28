// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

/// @dev Subset of the Hyperlane Mailbox v3 used as proof of delivery
///      (Mailbox.sol: `deliveries[id] = Delivery(msg.sender, uint48(block.number))`).
interface IMailboxDelivery {
    function processor(bytes32 _id) external view returns (address);

    function processedAt(bytes32 _id) external view returns (uint48);

    /// Dispatches a message over the bridge (receipt back). Pays the hook with msg.value.
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
 * @notice Vault beneficiary of the IGP (BSC/Ethereum) — spec §07. A relayer is
 *         not paid for being on a list: it is paid because `mailbox.processor(id)`
 *         says it was THEM who processed the message.
 *
 *         The upstream IGP `claim()` is permissionless and pushes the balance to the
 *         beneficiary — this contract — so there is NO Sweep: the payable
 *         `receive()` is enough. The relayer can call `igp.claim()` and
 *         `vault.claim(ids)` in the same transaction (its own multicall).
 *
 *         Owner = validators' multisig (with external signers, spec §04).
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

    /// fixed fee per proven delivery, in wei of the native currency
    uint256 public rewardPerDelivery;
    /// claim window in blocks, counted from the delivery block
    uint256 public claimWindowBlocks;
    bool public paused;

    /// message id → who claimed (zero address = not yet claimed)
    mapping(bytes32 id => address claimant) public claimedBy;

    uint256 public totalPaid;
    uint256 public totalClaims;

    uint256 private _entered; // reentrancy guard (1 = free, 2 = busy)

    // ---- v2 ClaimRemote: ORIGIN fee paid per attested remote delivery.
    //      This chain cannot see the others; trust rests on the set of
    //      attestors + bindings + quorum (same model as the Solana vault),
    //      with bounded damage: 1 payment per id, fixed reward per domain.
    struct RemoteClaimRecord {
        address executor;
        uint32 domain;
        uint256 amount;
        uint256 blockNumber;
    }

    address[] public remoteAttestors;
    mapping(address attestor => bool) public isRemoteAttestor;
    uint256 public remoteQuorum;
    /// local operator → remote domain → executor address there (e.g. terra1…)
    mapping(address operator => mapping(uint32 domain => string)) public remoteBinding;
    /// fixed reward per remote delivery, per domain (0 = disabled)
    mapping(uint32 domain => uint256) public remoteReward;
    /// message id → remote payment made (executor != 0 = paid; anti-double)
    mapping(bytes32 id => RemoteClaimRecord) public remoteClaimed;
    /// id → attestor → pointed executor (anti re-attestation)
    mapping(bytes32 id => mapping(address attestor => address executor)) public remoteVote;
    /// id → executor → number of agreeing attestations
    mapping(bytes32 id => mapping(address executor => uint256)) public remoteVoteCount;
    uint256 public totalRemotePaid;

    // ---- Phase 1 (trustless receipt): global mapping + routers ----
    /// domain of THIS vault (immutable, set in the constructor)
    uint32 public immutable localDomain;
    /// operator index → domain → address in that domain (string, multi-VM)
    mapping(uint32 index => mapping(uint32 domain => string)) public operatorAddress;
    /// reverse-lookup: LOCAL executor → operator index (+1; 0 = absent)
    mapping(address local => uint256 indexPlus1) internal _operatorOfLocalPlus1;
    /// trusted router (our vault) per domain, as bytes32 (Hyperlane convention)
    mapping(uint32 domain => bytes32 router) public remoteRouter;
    uint32 public operatorCount;
    /// ISM that validates the received receipts (the same as the route's warp). 0 = Mailbox
    /// default. Needed when the default ISM does not know the receipt's origin.
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

    /// @notice Required: this is where the IGP `claim()` deposits the collected funds.
    receive() external payable {
        emit Funded(msg.sender, msg.value);
    }

    // ============ Relayer ============

    /**
     * @notice Claims the reward for proven deliveries. ATOMIC: any invalid id
     *         reverts the whole batch (a duplicate in the batch falls into
     *         AlreadyClaimed, since the record is written inside the loop).
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

    /// @notice Owner: sets attestors and quorum of AGREEING attestations.
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

    /// @notice Owner: binds the operator's REMOTE address in a domain ("" removes).
    function setRemoteBinding(address operator, uint32 domain, string calldata remoteAddress)
        external
        onlyOwner
    {
        remoteBinding[operator][domain] = remoteAddress;
        emit RemoteBindingSet(operator, domain, remoteAddress);
    }

    /// @notice Owner: fixed reward per remote delivery in the domain (0 disables).
    function setRemoteReward(uint32 domain, uint256 reward) external onlyOwner {
        remoteReward[domain] = reward;
        emit RemoteRewardSet(domain, reward);
    }

    /**
     * @notice Attestor: asserts that the messages (dispatched FROM THIS mailbox to
     *         `domain` — the message id is the SAME on both chains) were delivered
     *         there by the address bound to `executor` (address(0) = the attestor
     *         itself). When the quorum of AGREEING attestations is reached it pays the
     *         reward — ONCE per id. ATOMIC: an invalid id reverts the batch.
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
            // ANTI-SELF-PAYMENT: with quorum >= 2, the vote of the beneficiary
            // ITSELF does NOT count — payment requires `quorum` INDEPENDENT attestors.
            // (records the vote to prevent re-voting, but does not advance the quorum.)
            uint256 agree = remoteVoteCount[id][exec];
            if (!(remoteQuorum >= 2 && msg.sender == exec)) {
                agree = ++remoteVoteCount[id][exec];
            }
            emit RemoteAttested(id, msg.sender, exec);
            if (agree >= remoteQuorum) {
                // effects-first: marks paid before the transfer
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

    // ---- Phase 1: global mapping + routers (owner only) ----

    /// @notice Writes the address of operator `index` in `domain` ("" removes). If
    ///         `domain` == localDomain, maintains the reverse-lookup (executor→index).
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

    /// @notice Index of the operator owning a LOCAL executor (0 = not registered).
    function operatorOfLocal(address local) external view returns (bool found, uint32 index) {
        uint256 p = _operatorOfLocalPlus1[local];
        return (p != 0, uint32(p == 0 ? 0 : p - 1));
    }

    /// @notice Trusted router (our vault) of a domain (bytes32(0) removes).
    function setRemoteRouter(uint32 domain, bytes32 router) external onlyOwner {
        remoteRouter[domain] = router;
        emit RemoteRouterSet(domain, router);
    }

    /// @notice Owner: ISM that validates the receipts (point it to the route's warp
    ///         ISM). 0 = uses the Mailbox's default ISM.
    function setIsm(address _ism) external onlyOwner {
        ism = _ism;
        emit IsmSet(_ism);
    }

    /// @dev converts the "0x…40hex" string into the address (for the local reverse-lookup).
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

    /// @notice How much these ids WOULD PAY if confirmed (not yet paid) — for
    ///         the operator to decide whether the receipt/attestation gas is worth it.
    ///         `amount` = payableCount × the domain's reward.
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

    // ============ Trustless receipt (Phase 2/3) ============

    /// @notice DESTINATION ROLE. Proves that these MESSAGES (full bytes) were
    ///         delivered HERE and dispatches ONE receipt back to the origin vault.
    ///         `id = keccak256(message)` and the origin domain is READ from the message
    ///         (bytes [1..5]) — the operator cannot forge the receipt's destination.
    ///         The operator pays the receipt gas via msg.value. All msgs in the batch
    ///         must have the SAME origin (one receipt → one origin vault).
    function sendReceipt(bytes[] calldata messages) external payable nonReentrant returns (bytes32) {
        if (paused) revert VaultPaused();
        uint256 n = messages.length;
        if (n == 0) revert EmptyBatch();

        uint32 originDomain = _originOf(messages[0]);
        bytes memory body = new bytes(n * 36); // id(32) + operatorIndex(4) per delivery
        for (uint256 i = 0; i < n; ++i) {
            if (_originOf(messages[i]) != originDomain) revert MixedOrigin();
            bytes32 id = keccak256(messages[i]);
            address exec = mailbox.processor(id);
            if (exec == address(0)) revert NotDelivered(id);
            (bool found, uint32 idx) = this.operatorOfLocal(exec);
            if (!found) revert UnknownExecutor(id, exec);
            // writes id + idx into the body (big-endian)
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

    /// @notice ORIGIN ROLE. Receives the receipt from the Mailbox. Only accepts from the
    ///         Mailbox itself and from a `sender` == the registered router of the origin domain.
    ///         Pays each (unpaid) id to the address of operator N in OUR registry
    ///         (localDomain). Idempotent: an already-paid id is ignored, does not revert.
    function handle(uint32 origin, bytes32 sender, bytes calldata body) external payable {
        if (msg.sender != address(mailbox)) revert NotMailbox();
        if (remoteRouter[origin] == bytes32(0) || sender != remoteRouter[origin]) {
            revert UntrustedRouter(origin, sender);
        }
        if (body.length == 0 || body.length % 36 != 0) revert MalformedReceipt();
        uint256 reward = remoteReward[origin]; // origin = where the delivery happened
        uint256 count = body.length / 36;
        for (uint256 i = 0; i < count; ++i) {
            uint256 off = i * 36;
            bytes32 id;
            assembly { id := calldataload(add(body.offset, off)) }
            if (remoteClaimed[id].executor != address(0)) continue; // idempotent
            uint32 idx = (uint32(uint8(body[off + 32])) << 24)
                | (uint32(uint8(body[off + 33])) << 16)
                | (uint32(uint8(body[off + 34])) << 8)
                | uint32(uint8(body[off + 35]));
            string memory payoutStr = operatorAddress[idx][localDomain];
            if (bytes(payoutStr).length == 0 || reward == 0) continue; // no registry/reward
            address payout = _parseAddr(payoutStr);
            remoteClaimed[id] = RemoteClaimRecord(payout, origin, reward, block.number);
            if (address(this).balance >= reward) {
                totalRemotePaid += reward;
                (bool ok, ) = payout.call{value: reward}("");
                if (!ok) revert TransferFailed();
                emit ReceiptPaid(id, idx, payout, reward);
            } else {
                emit ReceiptPaid(id, idx, payout, 0); // registered; pool unfunded (seed it)
            }
        }
    }

    /// @notice Recipient's ISM: the configured one (from the route's warp) or 0 = default.
    function interchainSecurityModule() external view returns (address) {
        return ism;
    }

    /// @dev origin domain of the Hyperlane msg: version(1)+nonce(4) → origin at [5..9].
    function _originOf(bytes calldata message) internal pure returns (uint32) {
        require(message.length >= 9, "msg");
        return (uint32(uint8(message[5])) << 24) | (uint32(uint8(message[6])) << 16)
            | (uint32(uint8(message[7])) << 8) | uint32(uint8(message[8]));
    }

    // ============ Views ============

    /// @notice How many deliveries the current pool can pay.
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

    /// two-step ownership — the new multisig must accept (avoids transferring to a dead address)
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
