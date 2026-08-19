// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {RelayerRewardVault} from "../src/RelayerRewardVault.sol";

/// Mock do Mailbox v3: espelha deliveries[id] = Delivery(processor, blockNumber).
contract MockMailbox {
    struct Delivery {
        address processor;
        uint48 blockNumber;
    }

    mapping(bytes32 => Delivery) public deliveries;

    function setDelivered(bytes32 id, address processor_, uint48 blockNumber) external {
        deliveries[id] = Delivery(processor_, blockNumber);
    }

    function processor(bytes32 id) external view returns (address) {
        return deliveries[id].processor;
    }

    function processedAt(bytes32 id) external view returns (uint48) {
        return deliveries[id].blockNumber;
    }

    // --- recibo (dispatch) ---
    uint32 public lastDest;
    bytes32 public lastRecipient;
    bytes public lastBody;
    uint256 public lastValue;

    function dispatch(uint32 destination, bytes32 recipient, bytes calldata body)
        external payable returns (bytes32)
    {
        lastDest = destination;
        lastRecipient = recipient;
        lastBody = body;
        lastValue = msg.value;
        return keccak256(abi.encode(destination, recipient, body));
    }

    function quoteDispatch(uint32, bytes32, bytes calldata) external pure returns (uint256) {
        return 0;
    }

    /// entrega um recibo no vault (simula o Mailbox chamando handle)
    function deliverHandle(address vault, uint32 origin, bytes32 sender, bytes memory body) external {
        (bool ok, ) = vault.call(abi.encodeWithSignature("handle(uint32,bytes32,bytes)", origin, sender, body));
        require(ok, "handle failed");
    }
}

/// Monta uma mensagem Hyperlane mínima com um origin domain específico (bytes[1..5]).
library MsgLib {
    function make(uint32 origin, uint32 nonce) internal pure returns (bytes memory m) {
        // version(1) + nonce(4) + origin(4) + sender(32) + dest(4) + recipient(32) + body
        m = abi.encodePacked(uint8(3), nonce, origin, bytes32(uint256(0x5eed)), uint32(56), bytes32(uint256(0xdead)), bytes("x"));
    }
}

/// Mock do IGP upstream: claim() é permissionless e empurra o saldo ao beneficiary.
contract MockIgp {
    address payable public beneficiary;

    constructor(address payable _beneficiary) {
        beneficiary = _beneficiary;
    }

    receive() external payable {}

    function claim() external {
        (bool ok, ) = beneficiary.call{value: address(this).balance}("");
        require(ok, "IGP: claim failed");
    }
}

/// Relayer malicioso que tenta reentrar no claim ao receber o pagamento.
contract ReentrantRelayer {
    RelayerRewardVault public vault;
    bytes32 public nextId;

    constructor(RelayerRewardVault _vault) {
        vault = _vault;
    }

    function attack(bytes32 first, bytes32 second) external {
        nextId = second;
        bytes32[] memory ids = new bytes32[](1);
        ids[0] = first;
        vault.claim(ids);
    }

    receive() external payable {
        bytes32[] memory ids = new bytes32[](1);
        ids[0] = nextId;
        vault.claim(ids); // deve falhar no guard
    }
}

contract RelayerRewardVaultTest is Test {
    uint256 constant REWARD = 0.01 ether;
    uint256 constant WINDOW = 100_000;

    MockMailbox mailbox;
    RelayerRewardVault vault;
    MockIgp igp;

    address multisig = makeAddr("multisig");
    address relayerA = makeAddr("relayerA");
    address relayerB = makeAddr("relayerB");

    function setUp() public {
        mailbox = new MockMailbox();
        vault = new RelayerRewardVault(address(mailbox), multisig, REWARD, WINDOW, 56);
        igp = new MockIgp(payable(address(vault)));
        vm.deal(address(igp), 1 ether); // arrecadação acumulada no IGP
    }

    function _fund(uint256 amount) internal {
        vm.deal(address(this), amount);
        (bool ok, ) = address(vault).call{value: amount}("");
        assertTrue(ok);
    }

    function _ids(bytes32 a) internal pure returns (bytes32[] memory arr) {
        arr = new bytes32[](1);
        arr[0] = a;
    }

    function test_constructor_validations() public {
        vm.expectRevert(RelayerRewardVault.ZeroAddress.selector);
        new RelayerRewardVault(address(0), multisig, REWARD, WINDOW, 56);
        vm.expectRevert(RelayerRewardVault.ZeroReward.selector);
        new RelayerRewardVault(address(mailbox), multisig, 0, WINDOW, 56);
        vm.expectRevert(RelayerRewardVault.ZeroWindow.selector);
        new RelayerRewardVault(address(mailbox), multisig, REWARD, 0, 56);
    }

    function test_claim_happy_path() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));

        vm.prank(relayerA);
        vault.claim(_ids(id));

        assertEq(relayerA.balance, REWARD);
        assertEq(vault.claimedBy(id), relayerA);
        assertEq(vault.totalPaid(), REWARD);
        assertEq(vault.totalClaims(), 1);
    }

    function test_claim_batch() public {
        _fund(1 ether);
        bytes32[] memory ids = new bytes32[](3);
        for (uint256 i = 0; i < 3; ++i) {
            ids[i] = keccak256(abi.encode("m", i));
            mailbox.setDelivered(ids[i], relayerA, uint48(block.number));
        }
        vm.prank(relayerA);
        vault.claim(ids);
        assertEq(relayerA.balance, 3 * REWARD);
    }

    function test_claim_not_delivered_reverts() public {
        _fund(1 ether);
        bytes32 id = keccak256("nope");
        vm.prank(relayerA);
        vm.expectRevert(abi.encodeWithSelector(RelayerRewardVault.NotDelivered.selector, id));
        vault.claim(_ids(id));
    }

    function test_claim_wrong_relayer_reverts() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));
        vm.prank(relayerB);
        vm.expectRevert(
            abi.encodeWithSelector(RelayerRewardVault.NotProcessor.selector, id, relayerA)
        );
        vault.claim(_ids(id));
    }

    function test_claim_twice_reverts() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));
        vm.prank(relayerA);
        vault.claim(_ids(id));

        vm.prank(relayerA);
        vm.expectRevert(
            abi.encodeWithSelector(RelayerRewardVault.AlreadyClaimed.selector, id, relayerA)
        );
        vault.claim(_ids(id));
    }

    function test_duplicate_in_batch_reverts() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));
        bytes32[] memory ids = new bytes32[](2);
        ids[0] = id;
        ids[1] = id;
        vm.prank(relayerA);
        vm.expectRevert(
            abi.encodeWithSelector(RelayerRewardVault.AlreadyClaimed.selector, id, relayerA)
        );
        vault.claim(ids);
    }

    function test_claim_window_expired_reverts() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        uint48 deliveredAt = uint48(block.number);
        mailbox.setDelivered(id, relayerA, deliveredAt);

        vm.roll(uint256(deliveredAt) + WINDOW + 1);
        vm.prank(relayerA);
        vm.expectRevert(
            abi.encodeWithSelector(
                RelayerRewardVault.ClaimWindowExpired.selector,
                id,
                uint256(deliveredAt) + WINDOW,
                block.number
            )
        );
        vault.claim(_ids(id));
    }

    function test_insufficient_pool_reverts_atomically() public {
        _fund(REWARD); // cobre só 1
        bytes32[] memory ids = new bytes32[](2);
        for (uint256 i = 0; i < 2; ++i) {
            ids[i] = keccak256(abi.encode("m", i));
            mailbox.setDelivered(ids[i], relayerA, uint48(block.number));
        }
        vm.prank(relayerA);
        vm.expectRevert(
            abi.encodeWithSelector(
                RelayerRewardVault.InsufficientPool.selector,
                2 * REWARD,
                REWARD
            )
        );
        vault.claim(ids);

        // atômico: nenhum id consumido
        assertEq(vault.claimedBy(ids[0]), address(0));
        assertEq(vault.claimedBy(ids[1]), address(0));
    }

    function test_igp_claim_then_vault_claim() public {
        // pool vazio; a arrecadação está no IGP. claim() do IGP é permissionless.
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));

        vm.prank(relayerA);
        vm.expectRevert(); // InsufficientPool
        vault.claim(_ids(id));

        vm.prank(relayerA);
        igp.claim(); // qualquer um pode; o dinheiro cai no vault (receive)
        assertEq(address(vault).balance, 1 ether);

        vm.prank(relayerA);
        vault.claim(_ids(id));
        assertEq(relayerA.balance, REWARD);
    }

    function test_pause_blocks_claim_and_is_owner_only() public {
        _fund(1 ether);
        bytes32 id = keccak256("m1");
        mailbox.setDelivered(id, relayerA, uint48(block.number));

        vm.prank(relayerA);
        vm.expectRevert(RelayerRewardVault.NotOwner.selector);
        vault.setPause(true);

        vm.prank(multisig);
        vault.setPause(true);

        vm.prank(relayerA);
        vm.expectRevert(RelayerRewardVault.VaultPaused.selector);
        vault.claim(_ids(id));

        vm.prank(multisig);
        vault.setPause(false);
        vm.prank(relayerA);
        vault.claim(_ids(id));
    }

    function test_set_params_owner_only() public {
        vm.prank(relayerA);
        vm.expectRevert(RelayerRewardVault.NotOwner.selector);
        vault.setParams(1, 1);

        vm.prank(multisig);
        vault.setParams(2 * REWARD, 5);
        assertEq(vault.rewardPerDelivery(), 2 * REWARD);
        assertEq(vault.claimWindowBlocks(), 5);
    }

    function test_withdraw_surplus_owner_only() public {
        _fund(1 ether);
        address treasury = makeAddr("treasury");

        vm.prank(relayerA);
        vm.expectRevert(RelayerRewardVault.NotOwner.selector);
        vault.withdrawSurplus(treasury, 0.5 ether);

        vm.prank(multisig);
        vault.withdrawSurplus(treasury, 0.5 ether);
        assertEq(treasury.balance, 0.5 ether);
    }

    function test_two_step_ownership() public {
        address newSig = makeAddr("newSig");
        vm.prank(multisig);
        vault.transferOwnership(newSig);
        assertEq(vault.owner(), multisig); // ainda não mudou

        vm.prank(relayerA);
        vm.expectRevert(RelayerRewardVault.NotPendingOwner.selector);
        vault.acceptOwnership();

        vm.prank(newSig);
        vault.acceptOwnership();
        assertEq(vault.owner(), newSig);
    }

    function test_reentrancy_is_blocked() public {
        _fund(1 ether);
        ReentrantRelayer attacker = new ReentrantRelayer(vault);
        bytes32 id1 = keccak256("m1");
        bytes32 id2 = keccak256("m2");
        mailbox.setDelivered(id1, address(attacker), uint48(block.number));
        mailbox.setDelivered(id2, address(attacker), uint48(block.number));

        // o reentrante falha no guard → o receive() reverte → TransferFailed
        vm.expectRevert(RelayerRewardVault.TransferFailed.selector);
        attacker.attack(id1, id2);
    }

    function test_claims_payable_view() public {
        _fund(5 * REWARD + 123);
        assertEq(vault.claimsPayable(), 5);
    }
}

// ===========================================================================
// v2 — ClaimRemote (atestação de entregas remotas)
// ===========================================================================
contract RelayerRewardVaultRemoteTest is Test {
    MockMailbox internal mailbox;
    RelayerRewardVault internal vault;
    address internal multisig = makeAddr("multisig");
    address internal operador = makeAddr("operador");
    address internal operador2 = makeAddr("operador2");
    uint32 internal constant DOM_TC = 132556;
    uint256 internal constant RREWARD = 0.0001 ether;

    function setUp() public {
        mailbox = new MockMailbox();
        vault = new RelayerRewardVault(address(mailbox), multisig, 1 ether, 1000, 56);
        vm.deal(address(vault), 10 ether);
        address[] memory atts = new address[](1);
        atts[0] = operador;
        vm.startPrank(multisig);
        vault.setRemoteOperators(atts, 1);
        vault.setRemoteBinding(operador, DOM_TC, "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp");
        vault.setRemoteReward(DOM_TC, RREWARD);
        vm.stopPrank();
    }

    function _ids1(bytes32 a) internal pure returns (bytes32[] memory ids) {
        ids = new bytes32[](1);
        ids[0] = a;
    }

    function test_quorum1_paga_na_hora() public {
        uint256 before = operador.balance;
        bytes32[] memory ids = new bytes32[](2);
        ids[0] = bytes32(uint256(0xA1));
        ids[1] = bytes32(uint256(0xA2));
        vm.prank(operador);
        vault.attestRemoteDelivery(DOM_TC, ids, address(0));
        assertEq(operador.balance, before + 2 * RREWARD);
        (address exec, uint32 dom, uint256 amt, ) = vault.remoteClaimed(ids[0]);
        assertEq(exec, operador);
        assertEq(dom, DOM_TC);
        assertEq(amt, RREWARD);
        assertEq(vault.totalRemotePaid(), 2 * RREWARD);
    }

    function test_id_nao_paga_duas_vezes() public {
        vm.prank(operador);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xB1))), address(0));
        vm.prank(operador);
        vm.expectRevert(
            abi.encodeWithSelector(
                RelayerRewardVault.RemoteAlreadyClaimed.selector, bytes32(uint256(0xB1)), operador
            )
        );
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xB1))), address(0));
    }

    function test_quorum2_exige_atestadores_independentes() public {
        address operador3 = makeAddr("operador3");
        address[] memory atts = new address[](3);
        atts[0] = operador; atts[1] = operador2; atts[2] = operador3;
        vm.prank(multisig);
        vault.setRemoteOperators(atts, 2);
        uint256 before = operador.balance;
        // o PRÓPRIO operador atesta a si — anti-autopagamento: NÃO conta
        vm.prank(operador);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xC1))), operador);
        assertEq(operador.balance, before);
        // 1º atestador independente voucha — ainda 1 de 2
        vm.prank(operador2);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xC1))), operador);
        assertEq(operador.balance, before);
        // 2º independente — fecha o quórum de INDEPENDENTES → paga
        vm.prank(operador3);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xC1))), operador);
        assertEq(operador.balance, before + RREWARD);
    }

    function test_autopagamento_bloqueado_em_quorum2() public {
        address[] memory atts = new address[](2);
        atts[0] = operador; atts[1] = operador2;
        vm.prank(multisig);
        vault.setRemoteOperators(atts, 2);
        uint256 before = operador.balance;
        // operador tenta se pagar sozinho — voto próprio não conta, nada pago
        vm.prank(operador);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xF2))), operador);
        assertEq(operador.balance, before);
        // e não pode votar de novo no mesmo id
        vm.prank(operador);
        vm.expectRevert(abi.encodeWithSelector(RelayerRewardVault.AlreadyAttested.selector, bytes32(uint256(0xF2)), operador));
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xF2))), operador);
    }

    function test_rejeita_nao_atestador_e_sem_vinculo_e_sem_recompensa() public {
        vm.prank(makeAddr("intruso"));
        vm.expectRevert(RelayerRewardVault.NotAttestor.selector);
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xD1))), address(0));

        vm.prank(operador);
        vm.expectRevert(
            abi.encodeWithSelector(RelayerRewardVault.NoBinding.selector, operador, uint32(99))
        );
        vault.attestRemoteDelivery(99, _ids1(bytes32(uint256(0xD2))), address(0));

        vm.prank(multisig);
        vault.setRemoteReward(DOM_TC, 0);
        vm.prank(operador);
        vm.expectRevert(
            abi.encodeWithSelector(RelayerRewardVault.NoRemoteReward.selector, DOM_TC)
        );
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xD3))), address(0));
    }

    function test_pool_insuficiente_reverte() public {
        vm.prank(multisig);
        vault.setRemoteReward(DOM_TC, 100 ether);
        vm.prank(operador);
        vm.expectRevert();
        vault.attestRemoteDelivery(DOM_TC, _ids1(bytes32(uint256(0xE1))), address(0));
    }

    function test_so_owner_configura() public {
        vm.prank(operador);
        vm.expectRevert(RelayerRewardVault.NotOwner.selector);
        vault.setRemoteReward(DOM_TC, 1);
    }
}

// ===========================================================================
// Fase 1 — registro de/para (EVM)
// ===========================================================================
contract RelayerRewardVaultRegistryTest is Test {
    MockMailbox internal mailbox;
    RelayerRewardVault internal vault;
    address internal multisig = makeAddr("multisig");
    address internal opTC = makeAddr("opTC");

    function setUp() public {
        mailbox = new MockMailbox();
        vault = new RelayerRewardVault(address(mailbox), multisig, 1 ether, 1000, 56); // BSC
    }

    function test_de_para_e_reverse_lookup_local() public {
        address execBsc = 0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291;
        vm.startPrank(multisig);
        // operador 0 no domínio LOCAL (BSC=56) → alimenta reverse-lookup
        vault.setOperatorAddress(0, 56, "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291");
        // e no TC (132556) → só de/para, sem reverse-lookup local
        vault.setOperatorAddress(0, 132556, "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp");
        vm.stopPrank();

        (bool found, uint32 idx) = vault.operatorOfLocal(execBsc);
        assertTrue(found);
        assertEq(idx, 0);
        assertEq(vault.operatorAddress(0, 132556), "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp");
        assertEq(vault.operatorCount(), 1);
    }

    function test_remover_limpa_reverse_lookup() public {
        address execBsc = 0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291;
        vm.startPrank(multisig);
        vault.setOperatorAddress(0, 56, "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291");
        vault.setOperatorAddress(0, 56, "");
        vm.stopPrank();
        (bool found, ) = vault.operatorOfLocal(execBsc);
        assertFalse(found);
    }

    function test_router_owner_only() public {
        vm.prank(opTC);
        vm.expectRevert(RelayerRewardVault.NotOwner.selector);
        vault.setRemoteRouter(132556, bytes32(uint256(1)));
        vm.prank(multisig);
        vault.setRemoteRouter(132556, bytes32(uint256(0xABCD)));
        assertEq(vault.remoteRouter(132556), bytes32(uint256(0xABCD)));
    }
}

// ===========================================================================
// Fase 2/3 — recibo trustless (EVM: sendReceipt no destino + handle na origem)
// ===========================================================================
contract RelayerRewardVaultReceiptTest is Test {
    using MsgLib for uint32;
    MockMailbox internal mailbox;
    RelayerRewardVault internal vault;   // faz papel de DESTINO (BSC) e ORIGEM
    address internal owner = makeAddr("owner");
    address internal exec = makeAddr("exec");      // executor local (entrega aqui)
    address internal payoutTC = makeAddr("payoutTC"); // conta do operador na "origem"
    uint32 internal constant ORIGIN = 132556;      // origem das msgs (TC)
    bytes32 internal constant ROUTER_TC = bytes32(uint256(0x7c));
    uint256 internal constant REWARD = 0.01 ether;

    function setUp() public {
        mailbox = new MockMailbox();
        vault = new RelayerRewardVault(address(mailbox), owner, 1 ether, 1000, 56); // BSC=56
        vm.deal(address(vault), 10 ether);
        vm.startPrank(owner);
        vault.setRemoteRouter(ORIGIN, ROUTER_TC);
        vault.setRemoteReward(ORIGIN, REWARD);
        // operador 0: executor local = exec (dom 56) e conta de pagamento (dom TC)
        vault.setOperatorAddress(0, 56, _hex(exec));
        vault.setOperatorAddress(0, ORIGIN, _hex(payoutTC)); // origem paga aqui se ELA fosse origem; aqui é só registro
        vm.stopPrank();
    }

    function _hex(address a) internal pure returns (string memory) {
        return vm.toString(a);
    }

    // DESTINO: sendReceipt prova entrega, lê origem da msg, despacha o recibo
    function test_sendReceipt_despacha_com_origem_lida_da_msg() public {
        bytes memory m = MsgLib.make(ORIGIN, 1);
        bytes32 id = keccak256(m);
        mailbox.setDelivered(id, exec, uint48(block.number)); // exec entregou aqui
        bytes[] memory msgs = new bytes[](1);
        msgs[0] = m;
        vm.prank(exec);
        vault.sendReceipt(msgs);
        assertEq(mailbox.lastDest(), ORIGIN);        // recibo vai p/ a origem lida da msg
        assertEq(mailbox.lastRecipient(), ROUTER_TC);
        assertEq(mailbox.lastBody().length, 36);     // 1 entrega
    }

    function test_sendReceipt_rejeita_nao_entregue_e_executor_desconhecido() public {
        bytes memory m = MsgLib.make(ORIGIN, 2);
        bytes[] memory msgs = new bytes[](1);
        msgs[0] = m;
        vm.expectRevert(abi.encodeWithSelector(RelayerRewardVault.NotDelivered.selector, keccak256(m)));
        vault.sendReceipt(msgs);
        // entregue por um executor SEM registro
        mailbox.setDelivered(keccak256(m), makeAddr("estranho"), uint48(block.number));
        vm.expectRevert();
        vault.sendReceipt(msgs);
    }

    // ORIGEM: handle paga o operador do PRÓPRIO registro, só do router confiável
    function test_handle_paga_operador_do_registro_local() public {
        // este vault agora faz papel de ORIGEM: dom local 56, recibo vindo do TC(132556)
        // registra o pagamento do operador 0 no dom LOCAL (56)
        vm.prank(owner);
        vault.setOperatorAddress(0, 56, _hex(payoutTC)); // no dom local paga payoutTC
        bytes32 id = keccak256(MsgLib.make(56, 9));
        bytes memory body = abi.encodePacked(id, uint32(0)); // (id, operador 0)
        uint256 before = payoutTC.balance;
        mailbox.deliverHandle(address(vault), ORIGIN, ROUTER_TC, body);
        assertEq(payoutTC.balance, before + REWARD);
        (address e, , uint256 amt, ) = vault.remoteClaimed(id);
        assertEq(e, payoutTC);
        assertEq(amt, REWARD);
    }

    function test_handle_rejeita_router_nao_confiavel_e_nao_mailbox() public {
        bytes memory body = abi.encodePacked(keccak256("x"), uint32(0));
        // não-mailbox
        vm.expectRevert(RelayerRewardVault.NotMailbox.selector);
        vault.handle(ORIGIN, ROUTER_TC, body);
        // router errado — chama direto COMO mailbox p/ o revert propagar
        vm.prank(address(mailbox));
        vm.expectRevert(abi.encodeWithSelector(RelayerRewardVault.UntrustedRouter.selector, ORIGIN, bytes32(uint256(0xbad))));
        vault.handle(ORIGIN, bytes32(uint256(0xbad)), body);
    }

    function test_handle_idempotente_nao_paga_duas_vezes() public {
        vm.prank(owner);
        vault.setOperatorAddress(0, 56, _hex(payoutTC));
        bytes32 id = keccak256(MsgLib.make(56, 7));
        bytes memory body = abi.encodePacked(id, uint32(0));
        mailbox.deliverHandle(address(vault), ORIGIN, ROUTER_TC, body);
        uint256 mid = payoutTC.balance;
        mailbox.deliverHandle(address(vault), ORIGIN, ROUTER_TC, body); // reentrega
        assertEq(payoutTC.balance, mid); // não pagou de novo
    }
}
