// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

/// @dev Subconjunto do StorageGasOracle do Hyperlane (OZ Ownable, passo único).
interface IStorageGasOracle {
    struct RemoteGasDataConfig {
        uint32 remoteDomain;
        uint128 tokenExchangeRate;
        uint128 gasPrice;
    }

    function setRemoteGasData(RemoteGasDataConfig calldata _config) external;

    function transferOwnership(address newOwner) external;
}

/**
 * @title GasOracleGovernor
 * @notice Torna-se owner do StorageGasOracle e reconstrói a separação de poderes
 *         (spec §07/§10): o MULTISIG define faixa, operadores, quórum e delta —
 *         e guarda os caminhos de emergência; os OPERADORES apenas submetem o
 *         preço observado. Ao bater o quórum na época (6h de block.timestamp),
 *         a MEDIANA (menor dos centrais em empate par — na dúvida cobra menos
 *         do usuário) é validada contra faixa + delta e escrita no oracle.
 */
contract GasOracleGovernor {
    // ============ Errors ============
    error NotOwner();
    error NotPendingOwner();
    error NotOperator();
    error NoBounds(uint32 domain);
    error InvalidBounds();
    error OutOfBounds(uint32 domain, uint256 value, uint256 min, uint256 max);
    error EpochAlreadyApplied(uint32 domain, uint256 epoch);
    error DeltaExceeded(uint32 domain, uint256 last, uint256 median, uint256 maxDeltaBps);
    error InvalidQuorum(uint256 operators);
    error ZeroEpochDuration();
    error ZeroAddress();
    error NoOperatorsLeft();

    // ============ Events ============
    event PriceSubmitted(
        uint32 indexed domain,
        uint256 indexed epoch,
        address indexed operator,
        uint128 tokenExchangeRate,
        uint128 gasPrice
    );
    event PriceApplied(
        uint32 indexed domain,
        uint256 indexed epoch,
        uint128 medianExchangeRate,
        uint128 medianGasPrice,
        bool forced
    );
    event BoundsSet(uint32 indexed domain);
    event BoundsUnset(uint32 indexed domain);
    event OperatorAdded(address indexed operator);
    event OperatorRemoved(address indexed operator);
    event QuorumSet(uint256 quorum);
    event EpochDurationSet(uint256 secs);
    event MaxDeltaBpsSet(uint256 bps);
    event OracleOwnershipTransferred(address indexed newOwner);
    event OwnershipTransferStarted(address indexed current, address indexed pending);
    event OwnershipTransferred(address indexed previous, address indexed current);

    // ============ Types ============
    struct Bounds {
        uint128 minExchangeRate;
        uint128 maxExchangeRate;
        uint128 minGasPrice;
        uint128 maxGasPrice;
        bool set;
    }

    struct Submission {
        uint128 tokenExchangeRate;
        uint128 gasPrice;
        bool set;
    }

    struct AppliedData {
        uint128 tokenExchangeRate;
        uint128 gasPrice;
        bool set;
        bool forced;
    }

    // ============ Storage ============
    IStorageGasOracle public immutable oracle;

    address public owner; // multisig
    address public pendingOwner;

    mapping(address => bool) public isOperator;
    uint256 public operatorCount;
    uint256 public quorum;
    uint256 public epochDurationSecs;
    uint256 public maxDeltaBps;

    mapping(uint32 domain => Bounds) public bounds;
    /// domínio → época → operador → submissão
    mapping(uint32 => mapping(uint256 => mapping(address => Submission))) public submissions;
    /// domínio → época → quem já submeteu (para varrer na mediana)
    mapping(uint32 => mapping(uint256 => address[])) public submitters;
    mapping(uint32 => mapping(uint256 => bool)) public applied;
    mapping(uint32 => AppliedData) public lastApplied;

    // ============ Modifiers ============
    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    // ============ Constructor ============
    constructor(
        address _oracle,
        address _owner,
        address[] memory _operators,
        uint256 _quorum,
        uint256 _epochDurationSecs,
        uint256 _maxDeltaBps
    ) {
        if (_oracle == address(0) || _owner == address(0)) revert ZeroAddress();
        if (_epochDurationSecs == 0) revert ZeroEpochDuration();
        oracle = IStorageGasOracle(_oracle);
        owner = _owner;
        epochDurationSecs = _epochDurationSecs;
        maxDeltaBps = _maxDeltaBps;

        for (uint256 i = 0; i < _operators.length; ++i) {
            address op = _operators[i];
            if (op == address(0)) revert ZeroAddress();
            if (!isOperator[op]) {
                isOperator[op] = true;
                operatorCount += 1;
                emit OperatorAdded(op);
            }
        }
        if (_quorum == 0 || _quorum > operatorCount) revert InvalidQuorum(operatorCount);
        quorum = _quorum;
        emit OwnershipTransferred(address(0), _owner);
    }

    // ============ Operadores ============

    function currentEpoch() public view returns (uint256) {
        return block.timestamp / epochDurationSecs;
    }

    function submitPrice(uint32 domain, uint128 tokenExchangeRate, uint128 gasPrice) external {
        if (!isOperator[msg.sender]) revert NotOperator();

        Bounds memory b = bounds[domain];
        if (!b.set) revert NoBounds(domain);
        _ensureInBounds(domain, tokenExchangeRate, b.minExchangeRate, b.maxExchangeRate);
        _ensureInBounds(domain, gasPrice, b.minGasPrice, b.maxGasPrice);

        uint256 epoch = currentEpoch();
        if (applied[domain][epoch]) revert EpochAlreadyApplied(domain, epoch);

        Submission storage existing = submissions[domain][epoch][msg.sender];
        if (!existing.set) {
            submitters[domain][epoch].push(msg.sender);
        }
        submissions[domain][epoch][msg.sender] = Submission(tokenExchangeRate, gasPrice, true);
        emit PriceSubmitted(domain, epoch, msg.sender, tokenExchangeRate, gasPrice);

        address[] memory who = submitters[domain][epoch];
        if (who.length < quorum) return;

        // mediana campo a campo (menor dos centrais no empate par)
        uint256 n = who.length;
        uint128[] memory rates = new uint128[](n);
        uint128[] memory gases = new uint128[](n);
        for (uint256 i = 0; i < n; ++i) {
            Submission memory s = submissions[domain][epoch][who[i]];
            rates[i] = s.tokenExchangeRate;
            gases[i] = s.gasPrice;
        }
        uint128 medianRate = _lowerMedian(rates);
        uint128 medianGas = _lowerMedian(gases);

        AppliedData memory last = lastApplied[domain];
        if (last.set) {
            _ensureDelta(domain, last.tokenExchangeRate, medianRate);
            _ensureDelta(domain, last.gasPrice, medianGas);
        }

        applied[domain][epoch] = true;
        lastApplied[domain] = AppliedData(medianRate, medianGas, true, false);
        emit PriceApplied(domain, epoch, medianRate, medianGas, false);

        oracle.setRemoteGasData(
            IStorageGasOracle.RemoteGasDataConfig(domain, medianRate, medianGas)
        );
    }

    // ============ Internos ============

    function _ensureInBounds(uint32 domain, uint128 value, uint128 min, uint128 max) internal pure {
        if (value < min || value > max) revert OutOfBounds(domain, value, min, max);
    }

    /// |novo − último| * 10_000 <= último * maxDeltaBps
    function _ensureDelta(uint32 domain, uint128 last, uint128 median) internal view {
        uint256 diff = median >= last ? median - last : last - median;
        if (diff * 10_000 > uint256(last) * maxDeltaBps) {
            revert DeltaExceeded(domain, last, median, maxDeltaBps);
        }
    }

    /// insertion sort em memória (n = quórum, pequeno) + índice (n-1)/2
    function _lowerMedian(uint128[] memory values) internal pure returns (uint128) {
        uint256 n = values.length;
        for (uint256 i = 1; i < n; ++i) {
            uint128 key = values[i];
            uint256 j = i;
            while (j > 0 && values[j - 1] > key) {
                values[j] = values[j - 1];
                --j;
            }
            values[j] = key;
        }
        return values[(n - 1) / 2];
    }

    // ============ Owner (multisig) ============

    function setBounds(uint32 domain, Bounds calldata b) external onlyOwner {
        if (
            !b.set ||
            b.minExchangeRate > b.maxExchangeRate ||
            b.minGasPrice > b.maxGasPrice ||
            b.maxExchangeRate == 0 ||
            b.maxGasPrice == 0
        ) revert InvalidBounds();
        bounds[domain] = b;
        emit BoundsSet(domain);
    }

    function unsetBounds(uint32 domain) external onlyOwner {
        delete bounds[domain];
        emit BoundsUnset(domain);
    }

    function setOperators(address[] calldata add, address[] calldata remove) external onlyOwner {
        for (uint256 i = 0; i < add.length; ++i) {
            address op = add[i];
            if (op == address(0)) revert ZeroAddress();
            if (!isOperator[op]) {
                isOperator[op] = true;
                operatorCount += 1;
                emit OperatorAdded(op);
            }
        }
        for (uint256 i = 0; i < remove.length; ++i) {
            address op = remove[i];
            if (isOperator[op]) {
                isOperator[op] = false;
                operatorCount -= 1;
                emit OperatorRemoved(op);
            }
        }
        if (operatorCount == 0) revert NoOperatorsLeft();
        if (quorum > operatorCount) revert InvalidQuorum(operatorCount);
    }

    function setQuorum(uint256 _quorum) external onlyOwner {
        if (_quorum == 0 || _quorum > operatorCount) revert InvalidQuorum(operatorCount);
        quorum = _quorum;
        emit QuorumSet(_quorum);
    }

    function setEpochDuration(uint256 secs) external onlyOwner {
        if (secs == 0) revert ZeroEpochDuration();
        epochDurationSecs = secs;
        emit EpochDurationSet(secs);
    }

    function setMaxDeltaBps(uint256 bps) external onlyOwner {
        maxDeltaBps = bps;
        emit MaxDeltaBpsSet(bps);
    }

    /// EMERGÊNCIA (spec §10): escrita direta, ignora quórum/faixa/delta e vira a
    /// nova base do delta.
    function forceSetRemoteGasData(
        uint32 domain,
        uint128 tokenExchangeRate,
        uint128 gasPrice
    ) external onlyOwner {
        lastApplied[domain] = AppliedData(tokenExchangeRate, gasPrice, true, true);
        emit PriceApplied(domain, currentEpoch(), tokenExchangeRate, gasPrice, true);
        oracle.setRemoteGasData(
            IStorageGasOracle.RemoteGasDataConfig(domain, tokenExchangeRate, gasPrice)
        );
    }

    /// SAÍDA DE EMERGÊNCIA: devolve a posse do oracle (OZ Ownable, passo único).
    function transferOracleOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        emit OracleOwnershipTransferred(newOwner);
        oracle.transferOwnership(newOwner);
    }

    // posse do próprio governor em dois passos
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

    // ============ Views auxiliares ============

    function submitterCount(uint32 domain, uint256 epoch) external view returns (uint256) {
        return submitters[domain][epoch].length;
    }
}
