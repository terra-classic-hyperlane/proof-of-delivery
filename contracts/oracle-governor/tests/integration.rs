//! Integração do oracle-governor com um mock do hpl-igp-oracle que espelha o
//! contrato real: posse em DOIS passos (InitOwnershipTransfer → ClaimOwnership)
//! e SetRemoteGasData restrito ao owner.

use cosmwasm_std::{Addr, Empty, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

use oracle_governor::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, SubmissionsResponse,
};
use oracle_governor::state::{AppliedGasData, Bounds};

const DOMAIN: u32 = 56; // BSC
const EPOCH_SECS: u64 = 21_600; // 6h
const DELTA_BPS: u64 = 2_000; // 20%

// ---------------------------------------------------------------------------
// Mock do hpl-igp-oracle
// ---------------------------------------------------------------------------
mod mock_oracle {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{
        to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError,
        StdResult, Uint128,
    };
    use cw_storage_plus::{Item, Map};

    pub const OWNER: Item<Addr> = Item::new("owner");
    pub const PENDING: Item<Option<Addr>> = Item::new("pending");
    /// domínio → (exchange_rate, gas_price)
    pub const DATA: Map<u32, (Uint128, Uint128)> = Map::new("data");

    #[cw_serde]
    pub struct InstantiateMsg {
        pub owner: String,
    }

    #[cw_serde]
    pub struct RemoteGasDataConfig {
        pub remote_domain: u32,
        pub token_exchange_rate: Uint128,
        pub gas_price: Uint128,
    }

    #[cw_serde]
    pub enum OwnableMsg {
        InitOwnershipTransfer { next_owner: String },
        RevokeOwnershipTransfer {},
        ClaimOwnership {},
    }

    #[cw_serde]
    pub enum ExecuteMsg {
        Ownership(OwnableMsg),
        SetRemoteGasDataConfigs { configs: Vec<RemoteGasDataConfig> },
        SetRemoteGasData { config: RemoteGasDataConfig },
    }

    #[cw_serde]
    pub enum QueryMsg {
        GetOwner {},
        GetGasData { domain: u32 },
    }

    pub fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<Response> {
        OWNER.save(deps.storage, &Addr::unchecked(msg.owner))?;
        PENDING.save(deps.storage, &None)?;
        Ok(Response::new())
    }

    pub fn execute(
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> StdResult<Response> {
        let owner = OWNER.load(deps.storage)?;
        match msg {
            ExecuteMsg::Ownership(o) => match o {
                OwnableMsg::InitOwnershipTransfer { next_owner } => {
                    if info.sender != owner {
                        return Err(StdError::generic_err("oracle: not owner"));
                    }
                    PENDING.save(deps.storage, &Some(Addr::unchecked(next_owner)))?;
                    Ok(Response::new())
                }
                OwnableMsg::RevokeOwnershipTransfer {} => {
                    if info.sender != owner {
                        return Err(StdError::generic_err("oracle: not owner"));
                    }
                    PENDING.save(deps.storage, &None)?;
                    Ok(Response::new())
                }
                OwnableMsg::ClaimOwnership {} => {
                    let pending = PENDING.load(deps.storage)?;
                    if pending.as_ref() != Some(&info.sender) {
                        return Err(StdError::generic_err("oracle: not pending owner"));
                    }
                    OWNER.save(deps.storage, &info.sender)?;
                    PENDING.save(deps.storage, &None)?;
                    Ok(Response::new())
                }
            },
            ExecuteMsg::SetRemoteGasData { config } => {
                if info.sender != owner {
                    return Err(StdError::generic_err("oracle: not owner"));
                }
                DATA.save(
                    deps.storage,
                    config.remote_domain,
                    &(config.token_exchange_rate, config.gas_price),
                )?;
                Ok(Response::new())
            }
            ExecuteMsg::SetRemoteGasDataConfigs { configs } => {
                if info.sender != owner {
                    return Err(StdError::generic_err("oracle: not owner"));
                }
                for c in configs {
                    DATA.save(
                        deps.storage,
                        c.remote_domain,
                        &(c.token_exchange_rate, c.gas_price),
                    )?;
                }
                Ok(Response::new())
            }
        }
    }

    pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::GetOwner {} => to_json_binary(&OWNER.load(deps.storage)?),
            QueryMsg::GetGasData { domain } => to_json_binary(&DATA.may_load(deps.storage, domain)?),
        }
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn governor_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        oracle_governor::entry::execute,
        oracle_governor::entry::instantiate,
        oracle_governor::entry::query,
    ))
}

fn oracle_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        mock_oracle::execute,
        mock_oracle::instantiate,
        mock_oracle::query,
    ))
}

struct Setup {
    app: App,
    governor: Addr,
    oracle: Addr,
    gov: Addr,
    ops: [Addr; 3],
}

/// Sobe oracle (owner=gov) + governor (3 operadores, quórum 2) e transfere a
/// posse do oracle para o governor pelo fluxo de dois passos.
fn setup() -> Setup {
    let gov = Addr::unchecked("gov");
    let ops = [
        Addr::unchecked("op_a"),
        Addr::unchecked("op_b"),
        Addr::unchecked("op_c"),
    ];

    let mut app = App::default();
    let oracle_code = app.store_code(oracle_contract());
    let governor_code = app.store_code(governor_contract());

    let oracle = app
        .instantiate_contract(
            oracle_code,
            gov.clone(),
            &mock_oracle::InstantiateMsg {
                owner: gov.to_string(),
            },
            &[],
            "oracle",
            None,
        )
        .unwrap();

    let governor = app
        .instantiate_contract(
            governor_code,
            gov.clone(),
            &InstantiateMsg {
                owner: gov.to_string(),
                oracle: oracle.to_string(),
                operators: ops.iter().map(|o| o.to_string()).collect(),
                quorum: 2,
                epoch_duration_secs: EPOCH_SECS,
                max_delta_bps: DELTA_BPS,
            },
            &[],
            "oracle-governor",
            None,
        )
        .unwrap();

    // posse do oracle → governor (passo 1 pela governança, passo 2 permissionless)
    app.execute_contract(
        gov.clone(),
        oracle.clone(),
        &mock_oracle::ExecuteMsg::Ownership(mock_oracle::OwnableMsg::InitOwnershipTransfer {
            next_owner: governor.to_string(),
        }),
        &[],
    )
    .unwrap();
    app.execute_contract(
        Addr::unchecked("anyone"),
        governor.clone(),
        &ExecuteMsg::ClaimOracleOwnership {},
        &[],
    )
    .unwrap();

    // faixa da governança para o domínio de teste
    app.execute_contract(
        gov.clone(),
        governor.clone(),
        &ExecuteMsg::SetBounds {
            domain: DOMAIN,
            bounds: Bounds {
                min_exchange_rate: Uint128::from(10u128),
                max_exchange_rate: Uint128::from(1_000u128),
                min_gas_price: Uint128::from(1u128),
                max_gas_price: Uint128::from(10_000u128),
            },
        },
        &[],
    )
    .unwrap();

    Setup {
        app,
        governor,
        oracle,
        gov,
        ops,
    }
}

fn submit(s: &mut Setup, op: &Addr, rate: u128, gas: u128) -> anyhow::Result<()> {
    s.app
        .execute_contract(
            op.clone(),
            s.governor.clone(),
            &ExecuteMsg::SubmitPrice {
                domain: DOMAIN,
                token_exchange_rate: Uint128::from(rate),
                gas_price: Uint128::from(gas),
            },
            &[],
        )
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!(e.root_cause().to_string()))
}

fn oracle_data(s: &Setup) -> Option<(Uint128, Uint128)> {
    s.app
        .wrap()
        .query_wasm_smart(&s.oracle, &mock_oracle::QueryMsg::GetGasData { domain: DOMAIN })
        .unwrap()
}

fn oracle_owner(s: &Setup) -> Addr {
    s.app
        .wrap()
        .query_wasm_smart(&s.oracle, &mock_oracle::QueryMsg::GetOwner {})
        .unwrap()
}

fn advance_epoch(s: &mut Setup) {
    s.app
        .update_block(|b| b.time = b.time.plus_seconds(EPOCH_SECS));
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[test]
fn instantiate_and_claim_oracle_ownership() {
    let s = setup();
    let cfg: ConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.governor, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.owner, s.gov);
    assert_eq!(cfg.oracle, s.oracle);
    assert_eq!(cfg.quorum, 2);
    assert_eq!(cfg.operator_count, 3);
    // a posse do oracle terminou no governor
    assert_eq!(oracle_owner(&s), s.governor);
}

#[test]
fn submit_by_non_operator_fails() {
    let mut s = setup();
    let outsider = Addr::unchecked("outsider");
    let err = submit(&mut s, &outsider, 100, 50).unwrap_err();
    assert!(err.to_string().contains("not a registered operator"));
}

#[test]
fn submit_without_bounds_fails() {
    let mut s = setup();
    let op = s.ops[0].clone();
    let err = s
        .app
        .execute_contract(
            op,
            s.governor.clone(),
            &ExecuteMsg::SubmitPrice {
                domain: 999, // sem faixa cadastrada
                token_exchange_rate: Uint128::from(100u128),
                gas_price: Uint128::from(50u128),
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("no bounds"));
}

#[test]
fn submit_out_of_bounds_fails() {
    let mut s = setup();
    let op = s.ops[0].clone();
    let err = submit(&mut s, &op, 5_000, 50).unwrap_err(); // rate > max 1000
    assert!(err.to_string().contains("out of bounds"));
}

#[test]
fn below_quorum_stores_but_does_not_apply() {
    let mut s = setup();
    let op = s.ops[0].clone();
    submit(&mut s, &op, 100, 50).unwrap();
    assert_eq!(oracle_data(&s), None); // nada aplicado

    let epoch: oracle_governor::msg::EpochResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.governor, &QueryMsg::CurrentEpoch {})
        .unwrap();
    let subs: SubmissionsResponse = s
        .app
        .wrap()
        .query_wasm_smart(
            &s.governor,
            &QueryMsg::Submissions {
                domain: DOMAIN,
                epoch: epoch.epoch,
            },
        )
        .unwrap();
    assert_eq!(subs.submissions.len(), 1);
}

#[test]
fn quorum_applies_median_to_oracle_odd() {
    let mut s = setup();
    // quórum é 2, mas deixamos os 3 submeterem na mesma época? Não: o 2º já aplica.
    // Para mediana de 3 valores, sobe o quórum para 3.
    let gov = s.gov.clone();
    s.app
        .execute_contract(
            gov,
            s.governor.clone(),
            &ExecuteMsg::SetQuorum { quorum: 3 },
            &[],
        )
        .unwrap();

    let (a, b, c) = (s.ops[0].clone(), s.ops[1].clone(), s.ops[2].clone());
    submit(&mut s, &a, 100, 10).unwrap();
    submit(&mut s, &b, 300, 30).unwrap();
    submit(&mut s, &c, 200, 20).unwrap(); // fecha o quórum

    // mediana de {100,200,300}=200 · {10,20,30}=20
    assert_eq!(
        oracle_data(&s),
        Some((Uint128::from(200u128), Uint128::from(20u128)))
    );
}

#[test]
fn even_quorum_uses_lower_central_median() {
    let mut s = setup(); // quórum 2
    let (a, b) = (s.ops[0].clone(), s.ops[1].clone());
    submit(&mut s, &a, 100, 10).unwrap();
    submit(&mut s, &b, 200, 40).unwrap();
    // par: usa o MENOR dos centrais — na dúvida, cobra menos
    assert_eq!(
        oracle_data(&s),
        Some((Uint128::from(100u128), Uint128::from(10u128)))
    );
}

#[test]
fn epoch_already_applied_rejects_more_submissions() {
    let mut s = setup();
    let (a, b, c) = (s.ops[0].clone(), s.ops[1].clone(), s.ops[2].clone());
    submit(&mut s, &a, 100, 10).unwrap();
    submit(&mut s, &b, 100, 10).unwrap(); // aplica
    let err = submit(&mut s, &c, 100, 10).unwrap_err();
    assert!(err.to_string().contains("already applied"));
}

#[test]
fn delta_exceeded_blocks_application() {
    let mut s = setup();
    let (a, b) = (s.ops[0].clone(), s.ops[1].clone());
    // época 1: estabelece a base = 100
    submit(&mut s, &a, 100, 100).unwrap();
    submit(&mut s, &b, 100, 100).unwrap();

    // época 2: salto de 30% > 2000 bps → a submissão que fecharia o quórum falha
    advance_epoch(&mut s);
    submit(&mut s, &a, 130, 100).unwrap();
    let err = submit(&mut s, &b, 130, 100).unwrap_err();
    assert!(err.to_string().contains("delta too large"));

    // dentro do limite (19%) passa — a submissão do op_a é sobrescrita e aplica
    submit(&mut s, &a, 119, 100).unwrap();
    submit(&mut s, &b, 119, 100).unwrap();
    assert_eq!(
        oracle_data(&s),
        Some((Uint128::from(119u128), Uint128::from(100u128)))
    );
}

#[test]
fn operator_overwrites_own_submission() {
    let mut s = setup();
    let a = s.ops[0].clone();
    submit(&mut s, &a, 100, 10).unwrap();
    submit(&mut s, &a, 150, 15).unwrap(); // sobrescreve, NÃO fecha quórum
    assert_eq!(oracle_data(&s), None);

    let b = s.ops[1].clone();
    submit(&mut s, &b, 150, 15).unwrap();
    // centrais {150,150} → 150
    assert_eq!(
        oracle_data(&s),
        Some((Uint128::from(150u128), Uint128::from(15u128)))
    );
}

#[test]
fn new_epoch_resets_submission_count() {
    let mut s = setup();
    let a = s.ops[0].clone();
    submit(&mut s, &a, 100, 10).unwrap();

    advance_epoch(&mut s);
    // submissão da época anterior não conta para a nova
    let b = s.ops[1].clone();
    submit(&mut s, &b, 100, 10).unwrap();
    assert_eq!(oracle_data(&s), None); // ainda 1 submissão nesta época
}

#[test]
fn admin_actions_are_owner_only() {
    let mut s = setup();
    let intruder = s.ops[0].clone(); // operador NÃO é governança

    for msg in [
        ExecuteMsg::SetQuorum { quorum: 1 },
        ExecuteMsg::SetMaxDeltaBps { max_delta_bps: 1 },
        ExecuteMsg::SetOperators {
            add: vec!["x".into()],
            remove: vec![],
        },
        ExecuteMsg::ForceSetRemoteGasData {
            domain: DOMAIN,
            token_exchange_rate: Uint128::from(1u128),
            gas_price: Uint128::from(1u128),
        },
        ExecuteMsg::InitOracleOwnershipTransfer {
            next_owner: "x".into(),
        },
    ] {
        let err = s
            .app
            .execute_contract(intruder.clone(), s.governor.clone(), &msg, &[])
            .unwrap_err();
        assert!(err.root_cause().to_string().contains("not the owner"));
    }
}

#[test]
fn force_set_writes_directly_and_resets_delta_base() {
    let mut s = setup();
    let gov = s.gov.clone();
    s.app
        .execute_contract(
            gov,
            s.governor.clone(),
            &ExecuteMsg::ForceSetRemoteGasData {
                domain: DOMAIN,
                token_exchange_rate: Uint128::from(500u128),
                gas_price: Uint128::from(700u128),
            },
            &[],
        )
        .unwrap();
    assert_eq!(
        oracle_data(&s),
        Some((Uint128::from(500u128), Uint128::from(700u128)))
    );

    let last: Option<AppliedGasData> = s
        .app
        .wrap()
        .query_wasm_smart(&s.governor, &QueryMsg::LastApplied { domain: DOMAIN })
        .unwrap();
    let last = last.unwrap();
    assert!(last.forced);
    assert_eq!(last.token_exchange_rate.u128(), 500);
}

#[test]
fn emergency_ownership_return_flow() {
    let mut s = setup();
    assert_eq!(oracle_owner(&s), s.governor);

    // governança manda o governor devolver a posse
    let gov = s.gov.clone();
    s.app
        .execute_contract(
            gov.clone(),
            s.governor.clone(),
            &ExecuteMsg::InitOracleOwnershipTransfer {
                next_owner: gov.to_string(),
            },
            &[],
        )
        .unwrap();
    // e reivindica direto no oracle
    s.app
        .execute_contract(
            gov.clone(),
            s.oracle.clone(),
            &mock_oracle::ExecuteMsg::Ownership(mock_oracle::OwnableMsg::ClaimOwnership {}),
            &[],
        )
        .unwrap();
    assert_eq!(oracle_owner(&s), gov);
}

#[test]
fn set_operators_respects_quorum_invariant() {
    let mut s = setup(); // 3 operadores, quórum 2
    let gov = s.gov.clone();

    // remover 2 deixaria 1 < quórum 2 → falha
    let err = s
        .app
        .execute_contract(
            gov.clone(),
            s.governor.clone(),
            &ExecuteMsg::SetOperators {
                add: vec![],
                remove: vec![s.ops[1].to_string(), s.ops[2].to_string()],
            },
            &[],
        )
        .unwrap_err();
    assert!(err.root_cause().to_string().contains("quorum"));

    // remover 1 (sobram 2 >= quórum) passa
    s.app
        .execute_contract(
            gov,
            s.governor.clone(),
            &ExecuteMsg::SetOperators {
                add: vec![],
                remove: vec![s.ops[2].to_string()],
            },
            &[],
        )
        .unwrap();
    let cfg: ConfigResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.governor, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(cfg.operator_count, 2);
}
