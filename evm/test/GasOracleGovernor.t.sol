// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {GasOracleGovernor, IStorageGasOracle} from "../src/GasOracleGovernor.sol";

/// Mock do StorageGasOracle: OZ Ownable de passo único + setRemoteGasData onlyOwner.
contract MockStorageGasOracle {
    address public owner;
    mapping(uint32 => IStorageGasOracle.RemoteGasDataConfig) public data;

    constructor(address _owner) {
        owner = _owner;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "oracle: not owner");
        _;
    }

    function setRemoteGasData(
        IStorageGasOracle.RemoteGasDataConfig calldata _config
    ) external onlyOwner {
        data[_config.remoteDomain] = _config;
    }

    function transferOwnership(address newOwner) external onlyOwner {
        owner = newOwner;
    }

    function get(uint32 domain) external view returns (uint128 rate, uint128 gas) {
        IStorageGasOracle.RemoteGasDataConfig memory c = data[domain];
        return (c.tokenExchangeRate, c.gasPrice);
    }
}

contract GasOracleGovernorTest is Test {
    uint32 constant DOMAIN = 56; // BSC
    uint256 constant EPOCH = 21_600; // 6h
    uint256 constant DELTA_BPS = 2_000; // 20%

    MockStorageGasOracle oracle;
    GasOracleGovernor governor;

    address multisig = makeAddr("multisig");
    address opA = makeAddr("opA");
    address opB = makeAddr("opB");
    address opC = makeAddr("opC");

    function setUp() public {
        oracle = new MockStorageGasOracle(address(this));

        address[] memory ops = new address[](3);
        ops[0] = opA;
        ops[1] = opB;
        ops[2] = opC;
        governor = new GasOracleGovernor(
            address(oracle),
            multisig,
            ops,
            2,
            EPOCH,
            DELTA_BPS
        );
        // posse do oracle → governor (checklist da spec §13)
        oracle.transferOwnership(address(governor));

        // faixa definida pelo multisig
        vm.prank(multisig);
        governor.setBounds(
            DOMAIN,
            GasOracleGovernor.Bounds(10, 1_000, 1, 10_000, true)
        );

        vm.warp(1_700_000_000); // timestamp realista
    }

    function _submit(address op, uint128 rate, uint128 gas) internal {
        vm.prank(op);
        governor.submitPrice(DOMAIN, rate, gas);
    }

    function test_constructor_invariants() public {
        address[] memory ops = new address[](1);
        ops[0] = opA;
        vm.expectRevert(abi.encodeWithSelector(GasOracleGovernor.InvalidQuorum.selector, 1));
        new GasOracleGovernor(address(oracle), multisig, ops, 2, EPOCH, DELTA_BPS);

        vm.expectRevert(GasOracleGovernor.ZeroEpochDuration.selector);
        new GasOracleGovernor(address(oracle), multisig, ops, 1, 0, DELTA_BPS);
    }

    function test_submit_by_non_operator_reverts() public {
        vm.prank(makeAddr("outsider"));
        vm.expectRevert(GasOracleGovernor.NotOperator.selector);
        governor.submitPrice(DOMAIN, 100, 50);
    }

    function test_submit_without_bounds_reverts() public {
        vm.prank(opA);
        vm.expectRevert(abi.encodeWithSelector(GasOracleGovernor.NoBounds.selector, uint32(999)));
        governor.submitPrice(999, 100, 50);
    }

    function test_submit_out_of_bounds_reverts() public {
        vm.prank(opA);
        vm.expectRevert(
            abi.encodeWithSelector(GasOracleGovernor.OutOfBounds.selector, DOMAIN, 5_000, 10, 1_000)
        );
        governor.submitPrice(DOMAIN, 5_000, 50);
    }

    function test_below_quorum_does_not_apply() public {
        _submit(opA, 100, 50);
        (uint128 rate, ) = oracle.get(DOMAIN);
        assertEq(rate, 0);
        assertEq(governor.submitterCount(DOMAIN, governor.currentEpoch()), 1);
    }

    function test_quorum_applies_median_odd() public {
        vm.prank(multisig);
        governor.setQuorum(3);

        _submit(opA, 100, 10);
        _submit(opB, 300, 30);
        _submit(opC, 200, 20);

        (uint128 rate, uint128 gas) = oracle.get(DOMAIN);
        assertEq(rate, 200); // mediana {100,200,300}
        assertEq(gas, 20);
    }

    function test_even_quorum_uses_lower_central() public {
        _submit(opA, 100, 10);
        _submit(opB, 200, 40);
        (uint128 rate, uint128 gas) = oracle.get(DOMAIN);
        assertEq(rate, 100); // menor dos centrais — na dúvida cobra menos
        assertEq(gas, 10);
    }

    function test_epoch_already_applied_rejects() public {
        _submit(opA, 100, 10);
        _submit(opB, 100, 10); // aplica

        uint256 epoch = governor.currentEpoch();
        vm.prank(opC);
        vm.expectRevert(
            abi.encodeWithSelector(GasOracleGovernor.EpochAlreadyApplied.selector, DOMAIN, epoch)
        );
        governor.submitPrice(DOMAIN, 100, 10);
    }

    function test_delta_exceeded_blocks_then_within_passes() public {
        // época 1: base 100
        _submit(opA, 100, 100);
        _submit(opB, 100, 100);

        // época 2: salto de 30% > 20% → bloqueia na submissão que fecharia o quórum
        vm.warp(block.timestamp + EPOCH);
        _submit(opA, 130, 100);
        vm.prank(opB);
        vm.expectRevert(
            abi.encodeWithSelector(
                GasOracleGovernor.DeltaExceeded.selector,
                DOMAIN,
                100,
                130,
                DELTA_BPS
            )
        );
        governor.submitPrice(DOMAIN, 130, 100);

        // 19% passa (opA sobrescreve a própria submissão)
        _submit(opA, 119, 100);
        _submit(opB, 119, 100);
        (uint128 rate, ) = oracle.get(DOMAIN);
        assertEq(rate, 119);
    }

    function test_operator_overwrites_own_submission() public {
        _submit(opA, 100, 10);
        _submit(opA, 150, 15); // sobrescreve, não fecha quórum
        (uint128 rate, ) = oracle.get(DOMAIN);
        assertEq(rate, 0);
        assertEq(governor.submitterCount(DOMAIN, governor.currentEpoch()), 1);

        _submit(opB, 150, 15);
        (rate, ) = oracle.get(DOMAIN);
        assertEq(rate, 150);
    }

    function test_new_epoch_resets_count() public {
        _submit(opA, 100, 10);
        vm.warp(block.timestamp + EPOCH);
        _submit(opB, 100, 10);
        (uint128 rate, ) = oracle.get(DOMAIN);
        assertEq(rate, 0); // 1 submissão em cada época — nenhum quórum
    }

    function test_admin_is_owner_only() public {
        vm.startPrank(opA); // operador NÃO é multisig
        vm.expectRevert(GasOracleGovernor.NotOwner.selector);
        governor.setQuorum(1);
        vm.expectRevert(GasOracleGovernor.NotOwner.selector);
        governor.setMaxDeltaBps(1);
        vm.expectRevert(GasOracleGovernor.NotOwner.selector);
        governor.forceSetRemoteGasData(DOMAIN, 1, 1);
        vm.expectRevert(GasOracleGovernor.NotOwner.selector);
        governor.transferOracleOwnership(opA);
        vm.stopPrank();
    }

    function test_force_set_writes_and_resets_delta_base() public {
        vm.prank(multisig);
        governor.forceSetRemoteGasData(DOMAIN, 500, 700);

        (uint128 rate, uint128 gas) = oracle.get(DOMAIN);
        assertEq(rate, 500);
        assertEq(gas, 700);

        (uint128 lastRate, , bool set, bool forced) = governor.lastApplied(DOMAIN);
        assertEq(lastRate, 500);
        assertTrue(set);
        assertTrue(forced);
    }

    function test_emergency_oracle_ownership_return() public {
        assertEq(oracle.owner(), address(governor));
        vm.prank(multisig);
        governor.transferOracleOwnership(multisig);
        assertEq(oracle.owner(), multisig);
    }

    function test_set_operators_respects_quorum() public {
        address[] memory none = new address[](0);
        address[] memory removeTwo = new address[](2);
        removeTwo[0] = opB;
        removeTwo[1] = opC;

        vm.prank(multisig);
        vm.expectRevert(abi.encodeWithSelector(GasOracleGovernor.InvalidQuorum.selector, 1));
        governor.setOperators(none, removeTwo);

        address[] memory removeOne = new address[](1);
        removeOne[0] = opC;
        vm.prank(multisig);
        governor.setOperators(none, removeOne);
        assertEq(governor.operatorCount(), 2);
    }

    function test_two_step_governor_ownership() public {
        address newSig = makeAddr("newSig");
        vm.prank(multisig);
        governor.transferOwnership(newSig);
        vm.prank(newSig);
        governor.acceptOwnership();
        assertEq(governor.owner(), newSig);
    }
}
