//! Integração com cw-multi-test: vault + mock do Mailbox (MESMO layout de storage
//! do hpl-mailbox, via cw-storage-plus) + mock do IGP (claim restrito ao beneficiary)
//! + mock de Mailbox "migrado" (layout alterado) para provar o MailboxLayoutMismatch.

use cosmwasm_std::{coin, coins, Addr, Empty, HexBinary, Uint128};
use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};

use relayer_reward_vault::msg::{
    ClaimedResponse, ConfigResponse, DeliveryResponse, ExecuteMsg, InstantiateMsg,
    LayoutCheckResponse, QueryMsg, SolvencyResponse,
};

const DENOM: &str = "uluna";
const REWARD: u128 = 1_000_000; // 1 LUNC por entrega (exemplo)
const WINDOW: u64 = 100_000;

// ---------------------------------------------------------------------------
// Mock do hpl-mailbox: grava DELIVERIES exatamente como o contrato real
// (Map::new("deliveries") com chave Vec<u8> e valor {sender, block_number}).
// ---------------------------------------------------------------------------
mod mock_mailbox {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{
        to_json_binary, Addr, Binary, Deps, DepsMut, Env, HexBinary, MessageInfo, Response,
        StdResult,
    };
    use cw_storage_plus::Map;

    #[cw_serde]
    pub struct Delivery {
        pub sender: Addr,
        pub block_number: u64,
    }

    pub const DELIVERIES: Map<Vec<u8>, Delivery> = Map::new("deliveries");

    #[cw_serde]
    pub struct InstantiateMsg {}

    #[cw_serde]
    pub enum ExecuteMsg {
        /// registra uma entrega como se `sender` tivesse executado o process()
        SetDelivery {
            message_id: HexBinary,
            sender: String,
            block_number: u64,
        },
    }

    pub fn instantiate(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: InstantiateMsg,
    ) -> StdResult<Response> {
        Ok(Response::new())
    }

    pub fn execute(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: ExecuteMsg,
    ) -> StdResult<Response> {
        match msg {
            ExecuteMsg::SetDelivery {
                message_id,
                sender,
                block_number,
            } => {
                DELIVERIES.save(
                    deps.storage,
                    message_id.to_vec(),
                    &Delivery {
                        sender: Addr::unchecked(sender),
                        block_number,
                    },
                )?;
                Ok(Response::new())
            }
        }
    }

    pub fn query(_deps: Deps, _env: Env, _msg: cosmwasm_std::Empty) -> StdResult<Binary> {
        to_json_binary(&())
    }
}

// ---------------------------------------------------------------------------
// Mock de Mailbox "MIGRADO": mesmo namespace, mas o valor ganhou um campo extra.
// O vault deve falhar com MailboxLayoutMismatch em vez de pagar.
// ---------------------------------------------------------------------------
mod mock_mailbox_migrated {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{
        to_json_binary, Addr, Binary, Deps, DepsMut, Env, HexBinary, MessageInfo, Response,
        StdResult,
    };
    use cw_storage_plus::Map;

    #[cw_serde]
    pub struct DeliveryV2 {
        pub sender: Addr,
        pub block_number: u64,
        pub gas_used: u64, // ← campo novo pós-"migrate"
    }

    pub const DELIVERIES: Map<Vec<u8>, DeliveryV2> = Map::new("deliveries");

    #[cw_serde]
    pub struct InstantiateMsg {}

    #[cw_serde]
    pub enum ExecuteMsg {
        SetDelivery {
            message_id: HexBinary,
            sender: String,
            block_number: u64,
        },
    }

    pub fn instantiate(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: InstantiateMsg,
    ) -> StdResult<Response> {
        Ok(Response::new())
    }

    pub fn execute(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: ExecuteMsg,
    ) -> StdResult<Response> {
        match msg {
            ExecuteMsg::SetDelivery {
                message_id,
                sender,
                block_number,
            } => {
                DELIVERIES.save(
                    deps.storage,
                    message_id.to_vec(),
                    &DeliveryV2 {
                        sender: Addr::unchecked(sender),
                        block_number,
                        gas_used: 123,
                    },
                )?;
                Ok(Response::new())
            }
        }
    }

    pub fn query(_deps: Deps, _env: Env, _msg: cosmwasm_std::Empty) -> StdResult<Binary> {
        to_json_binary(&())
    }
}

// ---------------------------------------------------------------------------
// Mock do hpl-igp: claim() SÓ pelo beneficiary; envia o saldo inteiro para ele.
// (espelha igps/core/src/execute.rs:90-103 do tc-cw-hyperlane)
// ---------------------------------------------------------------------------
mod mock_igp {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{
        to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError,
        StdResult,
    };
    use cw_storage_plus::Item;

    pub const BENEFICIARY: Item<String> = Item::new("beneficiary");

    #[cw_serde]
    pub struct InstantiateMsg {
        pub beneficiary: String,
    }

    #[cw_serde]
    pub enum ExecuteMsg {
        Claim {},
    }

    pub fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<Response> {
        BENEFICIARY.save(deps.storage, &msg.beneficiary)?;
        Ok(Response::new())
    }

    pub fn execute(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> StdResult<Response> {
        match msg {
            ExecuteMsg::Claim {} => {
                let beneficiary = BENEFICIARY.load(deps.storage)?;
                if info.sender != beneficiary {
                    return Err(StdError::generic_err("unauthorized: not beneficiary"));
                }
                let balance = deps
                    .querier
                    .query_balance(env.contract.address, super::DENOM)?;
                Ok(Response::new().add_message(BankMsg::Send {
                    to_address: beneficiary,
                    amount: vec![balance],
                }))
            }
        }
    }

    pub fn query(_deps: Deps, _env: Env, _msg: cosmwasm_std::Empty) -> StdResult<Binary> {
        to_json_binary(&())
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn vault_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        relayer_reward_vault::entry::execute,
        relayer_reward_vault::entry::instantiate,
        relayer_reward_vault::entry::query,
    ))
}

fn mailbox_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        mock_mailbox::execute,
        mock_mailbox::instantiate,
        mock_mailbox::query,
    ))
}

fn migrated_mailbox_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        mock_mailbox_migrated::execute,
        mock_mailbox_migrated::instantiate,
        mock_mailbox_migrated::query,
    ))
}

fn igp_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        mock_igp::execute,
        mock_igp::instantiate,
        mock_igp::query,
    ))
}

struct Setup {
    app: App,
    vault: Addr,
    mailbox: Addr,
    igp: Addr,
    gov: Addr,
    relayer_a: Addr,
    relayer_b: Addr,
}

/// Sobe mailbox + igp (beneficiary = vault) + vault; opcionalmente semeia o pool.
fn setup(pool_seed: u128) -> Setup {
    let gov = Addr::unchecked("gov");
    let relayer_a = Addr::unchecked("relayer_a");
    let relayer_b = Addr::unchecked("relayer_b");
    let funder = Addr::unchecked("funder");

    let mut app = AppBuilder::new().build(|router, _, storage| {
        router
            .bank
            .init_balance(storage, &funder, coins(1_000_000_000_000, DENOM))
            .unwrap();
    });

    let mailbox_code = app.store_code(mailbox_contract());
    let igp_code = app.store_code(igp_contract());
    let vault_code = app.store_code(vault_contract());

    let mailbox = app
        .instantiate_contract(
            mailbox_code,
            gov.clone(),
            &mock_mailbox::InstantiateMsg {},
            &[],
            "mailbox",
            None,
        )
        .unwrap();

    // vault precisa existir antes do IGP para ser o beneficiary?  Não: o IGP mock
    // recebe o beneficiary por string; instanciamos o vault primeiro com um igp
    // provisório e atualizamos depois seria pior — então: 1) igp com beneficiary
    // "placeholder", 2) vault, 3) UpdateConfig não é preciso: o IGP mock aceita
    // qualquer string; criamos o IGP DEPOIS do vault e passamos o endereço real,
    // e atualizamos o igp do vault via UpdateConfig do owner.
    let vault = app
        .instantiate_contract(
            vault_code,
            gov.clone(),
            &InstantiateMsg {
                owner: gov.to_string(),
                mailbox: mailbox.to_string(),
                igp: mailbox.to_string(), // provisório; corrigido logo abaixo
                denom: DENOM.to_string(),
                reward_per_delivery: Uint128::from(REWARD),
                claim_window_blocks: WINDOW,
            },
            &[],
            "relayer-reward-vault",
            None,
        )
        .unwrap();

    let igp = app
        .instantiate_contract(
            igp_code,
            gov.clone(),
            &mock_igp::InstantiateMsg {
                beneficiary: vault.to_string(),
            },
            &[],
            "igp",
            None,
        )
        .unwrap();

    app.execute_contract(
        gov.clone(),
        vault.clone(),
        &ExecuteMsg::UpdateConfig {
            owner: None,
            mailbox: None,
            igp: Some(igp.to_string()),
            reward_per_delivery: None,
            claim_window_blocks: None,
        },
        &[],
    )
    .unwrap();

    if pool_seed > 0 {
        app.send_tokens(funder.clone(), vault.clone(), &coins(pool_seed, DENOM))
            .unwrap();
    }
    // deixa o funder com utilidade p/ testes de IGP
    app.send_tokens(funder, igp.clone(), &coins(50_000_000, DENOM))
        .unwrap();

    Setup {
        app,
        vault,
        mailbox,
        igp,
        gov,
        relayer_a,
        relayer_b,
    }
}

fn msg_id(n: u8) -> HexBinary {
    HexBinary::from(vec![n; 32])
}

fn set_delivery(s: &mut Setup, id: &HexBinary, relayer: &Addr, block: u64) {
    let mailbox = s.mailbox.clone();
    let gov = s.gov.clone();
    s.app
        .execute_contract(
            gov,
            mailbox,
            &mock_mailbox::ExecuteMsg::SetDelivery {
                message_id: id.clone(),
                sender: relayer.to_string(),
                block_number: block,
            },
            &[],
        )
        .unwrap();
}

fn balance(app: &App, addr: &Addr) -> u128 {
    app.wrap().query_balance(addr, DENOM).unwrap().amount.u128()
}

fn current_height(app: &App) -> u64 {
    app.block_info().height
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[test]
fn instantiate_and_query_config() {
    let s = setup(0);
    let cfg: ConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.owner, s.gov);
    assert_eq!(cfg.mailbox, s.mailbox);
    assert_eq!(cfg.igp, s.igp);
    assert_eq!(cfg.denom, DENOM);
    assert_eq!(cfg.reward_per_delivery.u128(), REWARD);
    assert_eq!(cfg.claim_window_blocks, WINDOW);
    assert!(!cfg.paused);
    assert_eq!(cfg.total_paid, Uint128::zero());
    assert_eq!(cfg.total_claims, 0);
}

#[test]
fn claim_single_happy_path() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    let before = balance(&s.app, &s.relayer_a);
    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone()],
            },
            &[],
        )
        .unwrap();

    assert_eq!(balance(&s.app, &s.relayer_a), before + REWARD);

    let claimed: ClaimedResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::Claimed { message_id: id })
        .unwrap();
    assert!(claimed.claimed);
    assert_eq!(claimed.claimant.unwrap(), s.relayer_a);
    assert_eq!(claimed.amount.unwrap().u128(), REWARD);

    let cfg: ConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.total_paid.u128(), REWARD);
    assert_eq!(cfg.total_claims, 1);
}

#[test]
fn claim_batch_multiple() {
    let mut s = setup(10 * REWARD);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    let ids = [msg_id(1), msg_id(2), msg_id(3)];
    for id in &ids {
        set_delivery(&mut s, id, &relayer, h);
    }

    let before = balance(&s.app, &s.relayer_a);
    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: ids.to_vec(),
            },
            &[],
        )
        .unwrap();
    assert_eq!(balance(&s.app, &s.relayer_a), before + 3 * REWARD);
}

#[test]
fn claim_not_delivered_fails() {
    let mut s = setup(10 * REWARD);
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![msg_id(9)],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("not delivered"));
}

#[test]
fn claim_by_wrong_relayer_fails() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    let err = s
        .app
        .execute_contract(
            s.relayer_b.clone(), // ← não foi quem entregou
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("not by the claimer"));
}

#[test]
fn claim_twice_fails() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone()],
            },
            &[],
        )
        .unwrap();
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("already claimed"));
}

#[test]
fn claim_duplicate_in_batch_fails() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone(), id],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("duplicated"));
}

#[test]
fn claim_window_expired_fails() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    // avança além da janela
    s.app.update_block(|b| b.height += WINDOW + 1);

    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("window expired"));
}

#[test]
fn claim_insufficient_pool_fails_atomically() {
    // pool cobre só 1 entrega, lote pede 2 → nada é pago, nada fica marcado
    let mut s = setup(REWARD);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    let ids = [msg_id(1), msg_id(2)];
    for id in &ids {
        set_delivery(&mut s, id, &relayer, h);
    }

    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: ids.to_vec(),
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("insufficient pool"));

    // atômico: nenhum id ficou consumido
    for id in ids {
        let c: ClaimedResponse = s
            .app
            .wrap()
            .query_wasm_smart(&s.vault, &QueryMsg::Claimed { message_id: id })
            .unwrap();
        assert!(!c.claimed);
    }
}

#[test]
fn claim_batch_atomic_on_bad_id() {
    let mut s = setup(10 * REWARD);
    let good = msg_id(1);
    let bad = msg_id(2); // nunca entregue
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &good, &relayer, h);

    let before = balance(&s.app, &s.relayer_a);
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![good.clone(), bad],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("not delivered"));
    assert_eq!(balance(&s.app, &s.relayer_a), before);

    // o id válido NÃO foi consumido pelo lote revertido — segue resgatável
    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![good],
            },
            &[],
        )
        .unwrap();
    assert_eq!(balance(&s.app, &s.relayer_a), before + REWARD);
}

#[test]
fn sweep_is_permissionless_and_pulls_igp_funds() {
    let mut s = setup(0);
    let igp_before = balance(&s.app, &s.igp);
    assert!(igp_before > 0);
    let vault_before = balance(&s.app, &s.vault);

    // qualquer endereço aciona o Sweep
    let anyone = Addr::unchecked("anyone");
    s.app
        .execute_contract(anyone, s.vault.clone(), &ExecuteMsg::Sweep {}, &[])
        .unwrap();

    assert_eq!(balance(&s.app, &s.igp), 0);
    assert_eq!(balance(&s.app, &s.vault), vault_before + igp_before);
}

#[test]
fn sweep_then_claim_in_sequence() {
    // pool começa vazio; o Sweep enche com a arrecadação do IGP e o claim passa
    let mut s = setup(0);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone()],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("insufficient pool"));

    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Sweep {},
            &[],
        )
        .unwrap();
    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id],
            },
            &[],
        )
        .unwrap();
}

#[test]
fn migrated_mailbox_layout_is_detected_not_paid() {
    let mut s = setup(10 * REWARD);

    // sobe o mailbox "migrado" e aponta o vault para ele (via owner)
    let code = s.app.store_code(migrated_mailbox_contract());
    let migrated = s
        .app
        .instantiate_contract(
            code,
            s.gov.clone(),
            &mock_mailbox_migrated::InstantiateMsg {},
            &[],
            "mailbox-migrated",
            None,
        )
        .unwrap();
    s.app
        .execute_contract(
            s.gov.clone(),
            s.vault.clone(),
            &ExecuteMsg::UpdateConfig {
                owner: None,
                mailbox: Some(migrated.to_string()),
                igp: None,
                reward_per_delivery: None,
                claim_window_blocks: None,
            },
            &[],
        )
        .unwrap();

    let id = msg_id(7);
    let h = current_height(&s.app);
    s.app
        .execute_contract(
            s.gov.clone(),
            migrated.clone(),
            &mock_mailbox_migrated::ExecuteMsg::SetDelivery {
                message_id: id.clone(),
                sender: s.relayer_a.to_string(),
                block_number: h,
            },
            &[],
        )
        .unwrap();

    // claim falha explicitamente, sem pagar
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone()],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("layout mismatch"));

    // e o LayoutCheck reporta o problema para o monitoramento
    let check: LayoutCheckResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::LayoutCheck { message_id: id })
        .unwrap();
    assert!(!check.ok);
    assert!(check.detail.contains("VALUE LAYOUT MISMATCH"));
}

#[test]
fn layout_check_ok_on_real_layout() {
    let mut s = setup(0);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    let check: LayoutCheckResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::LayoutCheck { message_id: id })
        .unwrap();
    assert!(check.ok, "{}", check.detail);

    let d: DeliveryResponse = s
        .app
        .wrap()
        .query_wasm_smart(
            &s.vault,
            &QueryMsg::Delivery {
                message_id: msg_id(1),
            },
        )
        .unwrap();
    assert!(d.delivered);
    assert_eq!(d.processor.unwrap(), s.relayer_a);
}

#[test]
fn pause_blocks_claim_and_is_owner_only() {
    let mut s = setup(10 * REWARD);
    let id = msg_id(1);
    let h = current_height(&s.app);
    let relayer = s.relayer_a.clone();
    set_delivery(&mut s, &id, &relayer, h);

    // não-owner não pausa
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::SetPause { paused: true },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));

    // owner pausa → claim bloqueado
    s.app
        .execute_contract(
            s.gov.clone(),
            s.vault.clone(),
            &ExecuteMsg::SetPause { paused: true },
            &[],
        )
        .unwrap();
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id.clone()],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("paused"));

    // despausa → claim passa
    s.app
        .execute_contract(
            s.gov.clone(),
            s.vault.clone(),
            &ExecuteMsg::SetPause { paused: false },
            &[],
        )
        .unwrap();
    s.app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![id],
            },
            &[],
        )
        .unwrap();
}

#[test]
fn update_config_is_owner_only() {
    let mut s = setup(0);
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::UpdateConfig {
                owner: None,
                mailbox: None,
                igp: None,
                reward_per_delivery: Some(Uint128::from(2u128 * REWARD)),
                claim_window_blocks: None,
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));

    s.app
        .execute_contract(
            s.gov.clone(),
            s.vault.clone(),
            &ExecuteMsg::UpdateConfig {
                owner: None,
                mailbox: None,
                igp: None,
                reward_per_delivery: Some(Uint128::from(2u128 * REWARD)),
                claim_window_blocks: None,
            },
            &[],
        )
        .unwrap();
    let cfg: ConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.reward_per_delivery.u128(), 2 * REWARD);
}

#[test]
fn withdraw_surplus_is_owner_only_and_moves_funds() {
    let mut s = setup(10 * REWARD);
    let treasury = Addr::unchecked("treasury");

    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::WithdrawSurplus {
                to: treasury.to_string(),
                amount: Uint128::from(REWARD),
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));

    s.app
        .execute_contract(
            s.gov.clone(),
            s.vault.clone(),
            &ExecuteMsg::WithdrawSurplus {
                to: treasury.to_string(),
                amount: Uint128::from(REWARD),
            },
            &[],
        )
        .unwrap();
    assert_eq!(balance(&s.app, &treasury), REWARD);
}

#[test]
fn invalid_message_id_length_fails() {
    let mut s = setup(10 * REWARD);
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![HexBinary::from(vec![1u8; 31])],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("invalid message id"));
}

#[test]
fn empty_batch_fails() {
    let mut s = setup(10 * REWARD);
    let err = s
        .app
        .execute_contract(
            s.relayer_a.clone(),
            s.vault.clone(),
            &ExecuteMsg::Claim {
                message_ids: vec![],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("empty"));
}

#[test]
fn solvency_reports_capacity() {
    let s = setup(5 * REWARD + 123);
    let sol: SolvencyResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::Solvency {})
        .unwrap();
    assert_eq!(sol.pool, coin(5 * REWARD + 123, DENOM));
    assert_eq!(sol.claims_payable.u128(), 5);
}
