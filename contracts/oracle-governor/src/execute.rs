use cosmwasm_std::{
    ensure, to_json_binary, DepsMut, Env, MessageInfo, Order, Response, Uint128, Uint256, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::oracle_api::{OracleExecuteMsg, OwnableMsg, RemoteGasDataConfig};
use crate::state::{
    AppliedGasData, Bounds, Config, PriceSubmission, APPLIED, CONFIG, LAST_APPLIED,
    OPERATORS, OPERATOR_COUNT, SUBMISSIONS,
};

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(msg.epoch_duration_secs > 0, ContractError::ZeroEpoch {});
    ensure!(!msg.operators.is_empty(), ContractError::NoOperators {});

    let mut count: u32 = 0;
    for op in &msg.operators {
        let addr = deps.api.addr_validate(op)?;
        if !OPERATORS.has(deps.storage, &addr) {
            OPERATORS.save(deps.storage, &addr, &())?;
            count += 1;
        }
    }
    ensure!(
        msg.quorum >= 1 && msg.quorum <= count,
        ContractError::InvalidQuorum { operators: count }
    );

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        oracle: deps.api.addr_validate(&msg.oracle)?,
        epoch_duration_secs: msg.epoch_duration_secs,
        max_delta_bps: msg.max_delta_bps,
        quorum: msg.quorum,
    };
    CONFIG.save(deps.storage, &config)?;
    OPERATOR_COUNT.save(deps.storage, &count)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", config.owner)
        .add_attribute("oracle", config.oracle)
        .add_attribute("operators", count.to_string())
        .add_attribute("quorum", msg.quorum.to_string()))
}

pub fn current_epoch(env: &Env, config: &Config) -> u64 {
    env.block.time.seconds() / config.epoch_duration_secs
}

/// Median with "lower of the central ones" tie-break: sorts and takes index (n-1)/2.
/// Even → smaller of the two middle values (when in doubt, charge the user LESS — spec §10).
fn lower_median(values: &mut [Uint128]) -> Uint128 {
    values.sort();
    values[(values.len() - 1) / 2]
}

fn ensure_in_bounds(
    field: &str,
    value: Uint128,
    min: Uint128,
    max: Uint128,
    domain: u32,
) -> Result<(), ContractError> {
    ensure!(
        value >= min && value <= max,
        ContractError::OutOfBounds {
            field: field.to_string(),
            value: value.to_string(),
            min: min.to_string(),
            max: max.to_string(),
            domain,
        }
    );
    Ok(())
}

/// |new − last| * 10_000 <= last * max_delta_bps  (promoted to Uint256 to
/// never overflow). With no prior base, the first value passes freely.
fn ensure_delta(
    field: &str,
    last: Uint128,
    median: Uint128,
    max_delta_bps: u64,
    domain: u32,
) -> Result<(), ContractError> {
    let diff = if median >= last { median - last } else { last - median };
    let lhs = Uint256::from(diff) * Uint256::from(10_000u128);
    let rhs = Uint256::from(last) * Uint256::from(max_delta_bps as u128);
    ensure!(
        lhs <= rhs,
        ContractError::DeltaExceeded {
            field: field.to_string(),
            domain,
            last: last.to_string(),
            median: median.to_string(),
            max_delta_bps,
        }
    );
    Ok(())
}

pub fn submit_price(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    domain: u32,
    token_exchange_rate: Uint128,
    gas_price: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(
        OPERATORS.has(deps.storage, &info.sender),
        ContractError::NotOperator {}
    );

    // without governance bounds, the domain is locked for the operators
    let bounds = crate::state::BOUNDS
        .may_load(deps.storage, domain)?
        .ok_or(ContractError::NoBounds { domain })?;

    // immediate rejection of out-of-bounds submission (fail fast)
    ensure_in_bounds(
        "token_exchange_rate",
        token_exchange_rate,
        bounds.min_exchange_rate,
        bounds.max_exchange_rate,
        domain,
    )?;
    ensure_in_bounds(
        "gas_price",
        gas_price,
        bounds.min_gas_price,
        bounds.max_gas_price,
        domain,
    )?;

    let epoch = current_epoch(&env, &config);
    ensure!(
        !APPLIED.has(deps.storage, (domain, epoch)),
        ContractError::EpochAlreadyApplied { domain, epoch }
    );

    // overwrites the operator's own submission for the epoch, if any
    SUBMISSIONS.save(
        deps.storage,
        (domain, epoch, &info.sender),
        &PriceSubmission {
            token_exchange_rate,
            gas_price,
        },
    )?;

    // counts the epoch's submissions; once quorum is reached, applies the median
    let submissions: Vec<PriceSubmission> = SUBMISSIONS
        .prefix((domain, epoch))
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| item.map(|(_, sub)| sub))
        .collect::<Result<_, _>>()?;

    let mut response = Response::new()
        .add_attribute("action", "submit_price")
        .add_attribute("operator", info.sender.clone())
        .add_attribute("domain", domain.to_string())
        .add_attribute("epoch", epoch.to_string())
        .add_attribute("submissions", submissions.len().to_string());

    if (submissions.len() as u32) < config.quorum {
        return Ok(response.add_attribute("applied", "false"));
    }

    let mut rates: Vec<Uint128> = submissions.iter().map(|s| s.token_exchange_rate).collect();
    let mut gas_prices: Vec<Uint128> = submissions.iter().map(|s| s.gas_price).collect();
    let median_rate = lower_median(&mut rates);
    let median_gas = lower_median(&mut gas_prices);

    // the median of in-bounds values is in bounds, but we revalidate defensively
    ensure_in_bounds(
        "median token_exchange_rate",
        median_rate,
        bounds.min_exchange_rate,
        bounds.max_exchange_rate,
        domain,
    )?;
    ensure_in_bounds(
        "median gas_price",
        median_gas,
        bounds.min_gas_price,
        bounds.max_gas_price,
        domain,
    )?;

    if let Some(last) = LAST_APPLIED.may_load(deps.storage, domain)? {
        ensure_delta(
            "token_exchange_rate",
            last.token_exchange_rate,
            median_rate,
            config.max_delta_bps,
            domain,
        )?;
        ensure_delta(
            "gas_price",
            last.gas_price,
            median_gas,
            config.max_delta_bps,
            domain,
        )?;
    }

    let applied = AppliedGasData {
        token_exchange_rate: median_rate,
        gas_price: median_gas,
        epoch,
        forced: false,
    };
    APPLIED.save(deps.storage, (domain, epoch), &applied)?;
    LAST_APPLIED.save(deps.storage, domain, &applied)?;

    response = response
        .add_message(WasmMsg::Execute {
            contract_addr: config.oracle.to_string(),
            msg: to_json_binary(&OracleExecuteMsg::SetRemoteGasData {
                config: RemoteGasDataConfig {
                    remote_domain: domain,
                    token_exchange_rate: median_rate,
                    gas_price: median_gas,
                },
            })?,
            funds: vec![],
        })
        .add_attribute("applied", "true")
        .add_attribute("median_exchange_rate", median_rate)
        .add_attribute("median_gas_price", median_gas);

    Ok(response)
}

// ---------------------------------------------------------------------------
// Governance (owner)
// ---------------------------------------------------------------------------

fn ensure_owner(deps: &DepsMut, info: &MessageInfo) -> Result<Config, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    Ok(config)
}

pub fn set_bounds(
    deps: DepsMut,
    info: MessageInfo,
    domain: u32,
    bounds: Bounds,
) -> Result<Response, ContractError> {
    ensure_owner(&deps, &info)?;
    ensure!(
        bounds.min_exchange_rate <= bounds.max_exchange_rate
            && bounds.min_gas_price <= bounds.max_gas_price
            && !bounds.max_exchange_rate.is_zero()
            && !bounds.max_gas_price.is_zero(),
        ContractError::InvalidBounds {}
    );
    crate::state::BOUNDS.save(deps.storage, domain, &bounds)?;
    Ok(Response::new()
        .add_attribute("action", "set_bounds")
        .add_attribute("domain", domain.to_string()))
}

pub fn unset_bounds(
    deps: DepsMut,
    info: MessageInfo,
    domain: u32,
) -> Result<Response, ContractError> {
    ensure_owner(&deps, &info)?;
    crate::state::BOUNDS.remove(deps.storage, domain);
    Ok(Response::new()
        .add_attribute("action", "unset_bounds")
        .add_attribute("domain", domain.to_string()))
}

pub fn set_operators(
    deps: DepsMut,
    info: MessageInfo,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let config = ensure_owner(&deps, &info)?;
    let mut count = OPERATOR_COUNT.load(deps.storage)?;

    for op in add {
        let addr = deps.api.addr_validate(&op)?;
        if !OPERATORS.has(deps.storage, &addr) {
            OPERATORS.save(deps.storage, &addr, &())?;
            count += 1;
        }
    }
    for op in remove {
        let addr = deps.api.addr_validate(&op)?;
        if OPERATORS.has(deps.storage, &addr) {
            OPERATORS.remove(deps.storage, &addr);
            count -= 1;
        }
    }
    ensure!(count > 0, ContractError::NoOperators {});
    ensure!(
        config.quorum <= count,
        ContractError::InvalidQuorum { operators: count }
    );
    OPERATOR_COUNT.save(deps.storage, &count)?;
    Ok(Response::new()
        .add_attribute("action", "set_operators")
        .add_attribute("operators", count.to_string()))
}

pub fn set_quorum(deps: DepsMut, info: MessageInfo, quorum: u32) -> Result<Response, ContractError> {
    let mut config = ensure_owner(&deps, &info)?;
    let count = OPERATOR_COUNT.load(deps.storage)?;
    ensure!(
        quorum >= 1 && quorum <= count,
        ContractError::InvalidQuorum { operators: count }
    );
    config.quorum = quorum;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "set_quorum")
        .add_attribute("quorum", quorum.to_string()))
}

pub fn set_epoch_duration(
    deps: DepsMut,
    info: MessageInfo,
    epoch_duration_secs: u64,
) -> Result<Response, ContractError> {
    let mut config = ensure_owner(&deps, &info)?;
    ensure!(epoch_duration_secs > 0, ContractError::ZeroEpoch {});
    config.epoch_duration_secs = epoch_duration_secs;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "set_epoch_duration")
        .add_attribute("epoch_duration_secs", epoch_duration_secs.to_string()))
}

pub fn set_max_delta_bps(
    deps: DepsMut,
    info: MessageInfo,
    max_delta_bps: u64,
) -> Result<Response, ContractError> {
    let mut config = ensure_owner(&deps, &info)?;
    config.max_delta_bps = max_delta_bps;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "set_max_delta_bps")
        .add_attribute("max_delta_bps", max_delta_bps.to_string()))
}

pub fn set_owner(deps: DepsMut, info: MessageInfo, owner: String) -> Result<Response, ContractError> {
    let mut config = ensure_owner(&deps, &info)?;
    config.owner = deps.api.addr_validate(&owner)?;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "set_owner")
        .add_attribute("owner", config.owner))
}

/// EMERGENCY: direct write to the oracle by governance — ignores quorum, bounds
/// and delta, and becomes the new delta base.
pub fn force_set_remote_gas_data(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    domain: u32,
    token_exchange_rate: Uint128,
    gas_price: Uint128,
) -> Result<Response, ContractError> {
    let config = ensure_owner(&deps, &info)?;
    let epoch = current_epoch(&env, &config);
    let applied = AppliedGasData {
        token_exchange_rate,
        gas_price,
        epoch,
        forced: true,
    };
    LAST_APPLIED.save(deps.storage, domain, &applied)?;
    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.oracle.to_string(),
            msg: to_json_binary(&OracleExecuteMsg::SetRemoteGasData {
                config: RemoteGasDataConfig {
                    remote_domain: domain,
                    token_exchange_rate,
                    gas_price,
                },
            })?,
            funds: vec![],
        })
        .add_attribute("action", "force_set_remote_gas_data")
        .add_attribute("domain", domain.to_string()))
}

pub fn init_oracle_ownership_transfer(
    deps: DepsMut,
    info: MessageInfo,
    next_owner: String,
) -> Result<Response, ContractError> {
    let config = ensure_owner(&deps, &info)?;
    let next = deps.api.addr_validate(&next_owner)?;
    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.oracle.to_string(),
            msg: to_json_binary(&OracleExecuteMsg::Ownership(
                OwnableMsg::InitOwnershipTransfer {
                    next_owner: next.to_string(),
                },
            ))?,
            funds: vec![],
        })
        .add_attribute("action", "init_oracle_ownership_transfer")
        .add_attribute("next_owner", next))
}

pub fn revoke_oracle_ownership_transfer(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = ensure_owner(&deps, &info)?;
    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.oracle.to_string(),
            msg: to_json_binary(&OracleExecuteMsg::Ownership(
                OwnableMsg::RevokeOwnershipTransfer {},
            ))?,
            funds: vec![],
        })
        .add_attribute("action", "revoke_oracle_ownership_transfer"))
}

/// Step 2 of installation: with the governor as pending owner of the oracle, anyone
/// can trigger the Claim — the oracle only accepts if this contract is indeed the pending one.
pub fn claim_oracle_ownership(deps: DepsMut, _info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.oracle.to_string(),
            msg: to_json_binary(&OracleExecuteMsg::Ownership(OwnableMsg::ClaimOwnership {}))?,
            funds: vec![],
        })
        .add_attribute("action", "claim_oracle_ownership"))
}
