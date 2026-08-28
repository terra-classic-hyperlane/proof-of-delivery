//! Integration with cw-multi-test: vault + Mailbox mock (SAME storage layout
//! as hpl-mailbox, via cw-storage-plus) + IGP mock (claim restricted to the beneficiary)
//! + "migrated" Mailbox mock (altered layout) to prove the MailboxLayoutMismatch.

use cosmwasm_std::{coin, coins, Addr, Empty, HexBinary, Uint128};
use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};

use relayer_reward_vault::msg::{
    ClaimedResponse, ConfigResponse, DeliveryResponse, ExecuteMsg, InstantiateMsg,
    LayoutCheckResponse, QueryMsg, SolvencyResponse,
};

const DENOM: &str = "uluna";
const REWARD: u128 = 1_000_000; // 1 LUNC per delivery (example)
const WINDOW: u64 = 100_000;

// ---------------------------------------------------------------------------
// hpl-mailbox mock: writes DELIVERIES exactly like the real contract
// (Map::new("deliveries") with Vec<u8> key and {sender, block_number} value).
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

    use cw_storage_plus::Item;
    pub const LAST_DISPATCH: Item<(u32, HexBinary, HexBinary, Option<HexBinary>)> =
        Item::new("last_dispatch");

    #[cw_serde]
    pub struct DispatchMsg {
        pub dest_domain: u32,
        pub recipient_addr: HexBinary,
        pub msg_body: HexBinary,
        pub hook: Option<String>,
        pub metadata: Option<HexBinary>,
    }

    #[cw_serde]
    pub enum ExecuteMsg {
        /// registers a delivery as if `sender` had executed process()
        SetDelivery {
            message_id: HexBinary,
            sender: String,
            block_number: u64,
        },
        /// captures the receipt dispatched by the vault (destination role)
        Dispatch(DispatchMsg),
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
            ExecuteMsg::Dispatch(d) => {
                LAST_DISPATCH.save(
                    deps.storage,
                    &(d.dest_domain, d.recipient_addr, d.msg_body, d.metadata),
                )?;
                Ok(Response::new())
            }
        }
    }

    pub fn query(deps: Deps, _env: Env, _msg: QueryLastDispatch) -> StdResult<Binary> {
        to_json_binary(&LAST_DISPATCH.may_load(deps.storage)?)
    }

    #[cw_serde]
    pub struct QueryLastDispatch {}
}

// ---------------------------------------------------------------------------
// "MIGRATED" Mailbox mock: same namespace, but the value gained an extra field.
// The vault must fail with MailboxLayoutMismatch instead of paying.
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
        pub gas_used: u64, // ← new field post-"migrate"
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
// hpl-igp mock: claim() ONLY by the beneficiary; sends the entire balance to it.
// (mirrors igps/core/src/execute.rs:90-103 of tc-cw-hyperlane)
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

/// Brings up mailbox + igp (beneficiary = vault) + vault; optionally seeds the pool.
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

    // does the vault need to exist before the IGP to be the beneficiary?  No: the IGP mock
    // receives the beneficiary as a string; instantiating the vault first with a
    // provisional igp and updating later would be worse — so: 1) igp with a
    // "placeholder" beneficiary, 2) vault, 3) UpdateConfig is not needed: the IGP mock accepts
    // any string; we create the IGP AFTER the vault and pass the real address,
    // and we update the vault's igp via the owner's UpdateConfig.
    let vault = app
        .instantiate_contract(
            vault_code,
            gov.clone(),
            &InstantiateMsg {
                owner: gov.to_string(),
                mailbox: mailbox.to_string(),
                igp: mailbox.to_string(), // provisional; fixed right below
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
    // gives the funder a purpose for IGP tests
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
// Tests
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
            s.relayer_b.clone(), // ← was not the one who delivered
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

    // advances beyond the window
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
    // pool covers only 1 delivery, batch asks for 2 → nothing is paid, nothing is marked
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

    // atomic: no id was consumed
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
    let bad = msg_id(2); // never delivered
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

    // the valid id was NOT consumed by the reverted batch — remains claimable
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

    // any address triggers the Sweep
    let anyone = Addr::unchecked("anyone");
    s.app
        .execute_contract(anyone, s.vault.clone(), &ExecuteMsg::Sweep {}, &[])
        .unwrap();

    assert_eq!(balance(&s.app, &s.igp), 0);
    assert_eq!(balance(&s.app, &s.vault), vault_before + igp_before);
}

#[test]
fn sweep_then_claim_in_sequence() {
    // pool starts empty; the Sweep fills it with the IGP collection and the claim passes
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

    // brings up the "migrated" mailbox and points the vault at it (via owner)
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

    // claim fails explicitly, without paying
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

    // and the LayoutCheck reports the problem for monitoring
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

    // non-owner does not pause
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

    // owner pauses → claim blocked
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

    // unpauses → claim passes
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

// ===========================================================================
// v2 — ClaimRemote: attestation of remote deliveries
// ===========================================================================
use relayer_reward_vault::msg::{RemoteClaimedResponse, RemoteConfigResponse};

const DOM_SOL: u32 = 1_399_811_149;
const REMOTE_REWARD: u128 = 33_000_000; // 33 LUNC

/// enables remote mode: owner registers attestors/quorum, binding and reward
fn setup_remote(s: &mut Setup, attestors: &[&Addr], quorum: u32) {
    let owner = s.gov.clone();
    s.app
        .execute_contract(
            owner.clone(),
            s.vault.clone(),
            &ExecuteMsg::SetRemoteOperators {
                attestors: attestors.iter().map(|a| a.to_string()).collect(),
                quorum,
            },
            &[],
        )
        .unwrap();
    for a in attestors {
        s.app
            .execute_contract(
                owner.clone(),
                s.vault.clone(),
                &ExecuteMsg::SetRemoteBinding {
                    operator: a.to_string(),
                    domain: DOM_SOL,
                    remote_address: Some(format!("PbEo{}", a)),
                },
                &[],
            )
            .unwrap();
    }
    s.app
        .execute_contract(
            owner,
            s.vault.clone(),
            &ExecuteMsg::SetRemoteReward {
                domain: DOM_SOL,
                reward: Uint128::from(REMOTE_REWARD),
            },
            &[],
        )
        .unwrap();
}

#[test]
fn remote_quorum_1_paga_na_hora() {
    let mut s = setup(1_000_000_000);
    let att = s.relayer_a.clone();
    setup_remote(&mut s, &[&att], 1);
    let before = balance(&s.app, &att);
    s.app
        .execute_contract(
            att.clone(),
            s.vault.clone(),
            &ExecuteMsg::AttestRemoteDelivery {
                domain: DOM_SOL,
                message_ids: vec![msg_id(0xA1), msg_id(0xA2)],
                executor: None,
            },
            &[],
        )
        .unwrap();
    assert_eq!(balance(&s.app, &att), before + 2 * REMOTE_REWARD);
    let r: RemoteClaimedResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::RemoteClaimed { message_id: msg_id(0xA1) })
        .unwrap();
    assert!(r.claimed);
    assert_eq!(r.executor.unwrap(), att);
}

#[test]
fn remote_id_not_paga_duas_vezes() {
    let mut s = setup(1_000_000_000);
    let att = s.relayer_a.clone();
    setup_remote(&mut s, &[&att], 1);
    let msg = ExecuteMsg::AttestRemoteDelivery {
        domain: DOM_SOL,
        message_ids: vec![msg_id(0xB1)],
        executor: None,
    };
    s.app.execute_contract(att.clone(), s.vault.clone(), &msg, &[]).unwrap();
    let err = s.app.execute_contract(att.clone(), s.vault.clone(), &msg, &[]).unwrap_err();
    assert!(err.root_cause().to_string().contains("already paid"));
}

#[test]
fn remote_quorum_2_requires_independent_attesters() {
    let mut s = setup(1_000_000_000);
    let a1 = s.relayer_a.clone();
    let a2 = Addr::unchecked("operador2");
    let a3 = Addr::unchecked("operador3");
    setup_remote(&mut s, &[&a1, &a2, &a3], 2);
    let before = balance(&s.app, &a1);
    let att = |_who: &Addr, exec: &Addr| ExecuteMsg::AttestRemoteDelivery {
        domain: DOM_SOL,
        message_ids: vec![msg_id(0xC1)],
        executor: Some(exec.to_string()),
    };
    // a1 ITSELF attests to itself — anti-self-payment: does NOT count
    s.app.execute_contract(a1.clone(), s.vault.clone(), &att(&a1, &a1), &[]).unwrap();
    assert_eq!(balance(&s.app, &a1), before);
    // 1st independent (a2) — 1 of 2
    s.app.execute_contract(a2.clone(), s.vault.clone(), &att(&a2, &a1), &[]).unwrap();
    assert_eq!(balance(&s.app, &a1), before);
    // 2nd independent (a3) — closes the INDEPENDENTS' quorum → pays a1
    s.app.execute_contract(a3.clone(), s.vault.clone(), &att(&a3, &a1), &[]).unwrap();
    assert_eq!(balance(&s.app, &a1), before + REMOTE_REWARD);
}

#[test]
fn remote_rejects_non_attester_no_link_and_domain_without_reward() {
    let mut s = setup(1_000_000_000);
    let att = s.relayer_a.clone();
    setup_remote(&mut s, &[&att], 1);
    // non-attestor
    let err = s
        .app
        .execute_contract(
            Addr::unchecked("intruso"),
            s.vault.clone(),
            &ExecuteMsg::AttestRemoteDelivery {
                domain: DOM_SOL,
                message_ids: vec![msg_id(0xD1)],
                executor: None,
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("not a registered remote attestor"));
    // domain without reward
    let err = s
        .app
        .execute_contract(
            att.clone(),
            s.vault.clone(),
            &ExecuteMsg::AttestRemoteDelivery {
                domain: 56,
                message_ids: vec![msg_id(0xD2)],
                executor: None,
            },
            &[],
        )
        .unwrap_err();
    let texto = err.root_cause().to_string();
    assert!(texto.contains("no remote binding") || texto.contains("no remote reward"), "{texto}");
}

#[test]
fn remote_config_e_total_pago_consultaveis() {
    let mut s = setup(1_000_000_000);
    let att = s.relayer_a.clone();
    setup_remote(&mut s, &[&att], 1);
    s.app
        .execute_contract(
            att.clone(),
            s.vault.clone(),
            &ExecuteMsg::AttestRemoteDelivery {
                domain: DOM_SOL,
                message_ids: vec![msg_id(0xE1)],
                executor: None,
            },
            &[],
        )
        .unwrap();
    let rc: RemoteConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.vault, &QueryMsg::RemoteConfig {})
        .unwrap();
    assert_eq!(rc.quorum, 1);
    assert_eq!(rc.attestors, vec![att]);
    assert_eq!(rc.total_remote_paid, Uint128::from(REMOTE_REWARD));
}

// ===========================================================================
// Phase 1 — global from/to registry of operators + routers
// ===========================================================================
use relayer_reward_vault::msg::{
    OperatorAddressResponse, OperatorOfLocalResponse, RemoteRouterResponse,
};

const DOM_TC: u32 = 132_556;
const DOM_BSC: u32 = 56;

#[test]
fn registro_de_para_e_reverse_lookup() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    // operator 0: address on the TC (local) and on the BSC
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC,
            address: Some("terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp".into()) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_BSC,
            address: Some("0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291".into()) }, &[]).unwrap();

    // from/to: reads address by (index, domain)
    let r: OperatorAddressResponse = s.app.wrap().query_wasm_smart(&s.vault,
        &QueryMsg::OperatorAddress { index: 0, domain: DOM_BSC }).unwrap();
    assert_eq!(r.address.unwrap(), "0x8f085bad1a15ee9ceee58c83efffa72518975291"); // lowercase

    // reverse-lookup ONLY for the local domain (TC): executor terra1… → operator 0
    let rl: OperatorOfLocalResponse = s.app.wrap().query_wasm_smart(&s.vault,
        &QueryMsg::OperatorOfLocal { address: "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp".into() }).unwrap();
    assert_eq!(rl.index, Some(0));
    // remote address (BSC) does NOT enter the local reverse-lookup
    let rl2: OperatorOfLocalResponse = s.app.wrap().query_wasm_smart(&s.vault,
        &QueryMsg::OperatorOfLocal { address: "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291".into() }).unwrap();
    assert_eq!(rl2.index, None);
}

#[test]
fn remover_operador_limpa_reverse_lookup() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    let set = |s: &mut Setup, a: Option<String>| s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC, address: a }, &[]).unwrap();
    set(&mut s, Some("terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp".into()));
    set(&mut s, None);
    let rl: OperatorOfLocalResponse = s.app.wrap().query_wasm_smart(&s.vault,
        &QueryMsg::OperatorOfLocal { address: "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp".into() }).unwrap();
    assert_eq!(rl.index, None);
}

#[test]
fn router_owner_only() {
    let mut s = setup(1_000_000_000);
    let err = s.app.execute_contract(Addr::unchecked("intruso"), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC, address: Some("0xabc".into()) }, &[]).unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));
    // owner writes and reads
    s.app.execute_contract(s.gov.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC, address: Some("0x1A41144c".into()) }, &[]).unwrap();
    let r: RemoteRouterResponse = s.app.wrap().query_wasm_smart(&s.vault,
        &QueryMsg::RemoteRouter { domain: DOM_BSC }).unwrap();
    assert_eq!(r.address.unwrap(), "0x1a41144c");
}

// ===========================================================================
// Phase 2/3 — trustless receipt (CW: send_receipt on destination + handle on origin)
// ===========================================================================
use sha3::{Digest, Keccak256};

const DOM_BSC_R: u32 = 56;

/// builds a Hyperlane message with origin_domain embedded in [5..9]
fn hyp_msg(origin: u32, nonce: u32) -> HexBinary {
    let mut m = vec![3u8]; // version
    m.extend_from_slice(&nonce.to_be_bytes());
    m.extend_from_slice(&origin.to_be_bytes());
    m.extend_from_slice(&[0u8; 32]); // sender
    m.extend_from_slice(&DOM_TC.to_be_bytes()); // dest
    m.extend_from_slice(&[0u8; 32]); // recipient
    m.extend_from_slice(b"x"); // body
    HexBinary::from(m)
}
fn keccak_id(m: &HexBinary) -> HexBinary {
    let mut h = Keccak256::new();
    h.update(m.as_slice());
    HexBinary::from(h.finalize().to_vec())
}

#[test]
fn send_receipt_proves_delivery_and_dispatches() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    let relayer = s.relayer_a.clone();
    // registry: local executor (relayer_a) = operator 0; BSC router
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC, address: Some(relayer.to_string()) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC_R,
            address: Some("0x00000000000000000000000000000000000000000000000000000000000000bc".into()) }, &[]).unwrap();
    // message originated on the BSC (56), delivered HERE (TC) by relayer_a
    let m = hyp_msg(DOM_BSC_R, 1);
    let id = keccak_id(&m);
    s.app.execute_contract(relayer.clone(), s.mailbox.clone(),
        &mock_mailbox::ExecuteMsg::SetDelivery { message_id: id.clone(), sender: relayer.to_string(), block_number: 100 }, &[]).unwrap();
    // send_receipt (destination role) — dispatches 1 receipt to the BSC
    s.app.execute_contract(relayer.clone(), s.vault.clone(),
        &ExecuteMsg::SendReceipt { messages: vec![m], gas_limit: None }, &[]).unwrap();
    // the mock mailbox captured the dispatch: destination 56, 36-byte body
    let last: Option<(u32, HexBinary, HexBinary, Option<HexBinary>)> = s.app.wrap()
        .query_wasm_smart(&s.mailbox, &mock_mailbox::QueryLastDispatch {}).unwrap();
    let (dest, _router, body, metadata) = last.unwrap();
    assert_eq!(dest, DOM_BSC_R);
    assert_eq!(body.len(), 36);
    assert_eq!(&body.as_slice()[0..32], id.as_slice()); // id in the body
    assert_eq!(metadata, None); // no gas_limit → no metadata (IGP uses gas_for_domain)
}

#[test]
fn send_receipt_com_gas_limit_vira_metadata_do_igp() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    let relayer = s.relayer_a.clone();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC, address: Some(relayer.to_string()) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC_R,
            address: Some("0x00000000000000000000000000000000000000000000000000000000000000bc".into()) }, &[]).unwrap();
    let m = hyp_msg(DOM_BSC_R, 3);
    let id = keccak_id(&m);
    s.app.execute_contract(relayer.clone(), s.mailbox.clone(),
        &mock_mailbox::ExecuteMsg::SetDelivery { message_id: id.clone(), sender: relayer.to_string(), block_number: 100 }, &[]).unwrap();
    // gas_limit → IGP metadata: 32 BE bytes of the value (receipt pays only real gas)
    s.app.execute_contract(relayer.clone(), s.vault.clone(),
        &ExecuteMsg::SendReceipt { messages: vec![m], gas_limit: Some(cosmwasm_std::Uint256::from(300_000u32)) }, &[]).unwrap();
    let last: Option<(u32, HexBinary, HexBinary, Option<HexBinary>)> = s.app.wrap()
        .query_wasm_smart(&s.mailbox, &mock_mailbox::QueryLastDispatch {}).unwrap();
    let (_dest, _router, _body, metadata) = last.unwrap();
    let md = metadata.expect("gas_limit must produce metadata");
    assert_eq!(md.len(), 32); // only the gas_limit; empty refund → refund = the vault (pool)
    assert_eq!(md.as_slice(), cosmwasm_std::Uint256::from(300_000u32).to_be_bytes().as_slice());
}

#[test]
fn send_receipt_not_reemite_o_mesmo_id() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    let relayer = s.relayer_a.clone();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC, address: Some(relayer.to_string()) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC_R,
            address: Some("0x00000000000000000000000000000000000000000000000000000000000000bc".into()) }, &[]).unwrap();
    let m = hyp_msg(DOM_BSC_R, 7);
    let id = keccak_id(&m);
    s.app.execute_contract(relayer.clone(), s.mailbox.clone(),
        &mock_mailbox::ExecuteMsg::SetDelivery { message_id: id.clone(), sender: relayer.to_string(), block_number: 100 }, &[]).unwrap();
    // 1st issuance: ok
    s.app.execute_contract(relayer.clone(), s.vault.clone(),
        &ExecuteMsg::SendReceipt { messages: vec![m.clone()], gas_limit: None }, &[]).unwrap();
    // 2nd issuance of the SAME id: refused (nothing new) — anti-double-payment on destination
    let err = s.app.execute_contract(relayer.clone(), s.vault.clone(),
        &ExecuteMsg::SendReceipt { messages: vec![m], gas_limit: None }, &[]).unwrap_err();
    assert!(err.root_cause().to_string().contains("nothing new to send"));
}

#[test]
fn handle_paga_operador_do_registro_local_e_idempotente() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    let payout = s.relayer_a.clone();
    let router_bsc = "0x00000000000000000000000000000000000000000000000000000000000000bc";
    // operator 0 receives in the LOCAL domain (TC); reward and router for BSC origin
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetOperatorAddress { index: 0, domain: DOM_TC, address: Some(payout.to_string()) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteReward { domain: DOM_BSC_R, reward: Uint128::from(33_000_000u128) }, &[]).unwrap();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC_R, address: Some(router_bsc.into()) }, &[]).unwrap();

    let id = keccak_id(&hyp_msg(DOM_BSC_R, 9));
    let mut body = id.to_vec();
    body.extend_from_slice(&0u32.to_be_bytes()); // operator 0
    let handle = ExecuteMsg::Handle(relayer_reward_vault::msg::HandleMsg {
        origin: DOM_BSC_R,
        sender: HexBinary::from(hex::decode("00000000000000000000000000000000000000000000000000000000000000bc").unwrap()),
        body: HexBinary::from(body),
    });
    let before = balance(&s.app, &payout);
    // ONLY the mailbox calls handle
    s.app.execute_contract(s.mailbox.clone(), s.vault.clone(), &handle, &[]).unwrap();
    assert_eq!(balance(&s.app, &payout), before + 33_000_000);
    // redelivery of the SAME receipt does not pay again
    s.app.execute_contract(s.mailbox.clone(), s.vault.clone(), &handle, &[]).unwrap();
    assert_eq!(balance(&s.app, &payout), before + 33_000_000);
}

#[test]
fn handle_rejects_non_mailbox_and_wrong_router() {
    let mut s = setup(1_000_000_000);
    let owner = s.gov.clone();
    s.app.execute_contract(owner.clone(), s.vault.clone(),
        &ExecuteMsg::SetRemoteRouter { domain: DOM_BSC_R,
            address: Some("0x00000000000000000000000000000000000000000000000000000000000000bc".into()) }, &[]).unwrap();
    let mut body = keccak_id(&hyp_msg(DOM_BSC_R, 1)).to_vec();
    body.extend_from_slice(&0u32.to_be_bytes());
    let good = HexBinary::from(hex::decode("00000000000000000000000000000000000000000000000000000000000000bc").unwrap());
    // non-mailbox (any relayer) → Unauthorized
    let err = s.app.execute_contract(s.relayer_a.clone(), s.vault.clone(),
        &ExecuteMsg::Handle(relayer_reward_vault::msg::HandleMsg { origin: DOM_BSC_R, sender: good.clone(), body: HexBinary::from(body.clone()) }), &[]).unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));
    // mailbox, but sender != router → Unauthorized
    let bad = HexBinary::from(hex::decode("00000000000000000000000000000000000000000000000000000000000000ff").unwrap());
    let err = s.app.execute_contract(s.mailbox.clone(), s.vault.clone(),
        &ExecuteMsg::Handle(relayer_reward_vault::msg::HandleMsg { origin: DOM_BSC_R, sender: bad, body: HexBinary::from(body) }), &[]).unwrap_err();
    assert!(err.root_cause().to_string().contains("unauthorized"));
}

#[test]
fn ism_specifier_query_deserializa_o_json_do_mailbox() {
    // the hpl-mailbox sends exactly: {"ism_specifier":{"interchain_security_module":[]}}
    let json = br#"{"ism_specifier":{"interchain_security_module":[]}}"#;
    let parsed: relayer_reward_vault::msg::QueryMsg = cosmwasm_std::from_json(json).unwrap();
    matches!(parsed, relayer_reward_vault::msg::QueryMsg::IsmSpecifier(_));
}
