use cosmwasm_std::{Deps, Env, Order};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, EpochResponse, OperatorsResponse, SubmissionEntry, SubmissionsResponse,
};
use crate::state::{
    AppliedGasData, Bounds, APPLIED, BOUNDS, CONFIG, LAST_APPLIED, OPERATORS, OPERATOR_COUNT,
    SUBMISSIONS,
};

pub fn config(deps: Deps) -> Result<ConfigResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner,
        oracle: config.oracle,
        epoch_duration_secs: config.epoch_duration_secs,
        max_delta_bps: config.max_delta_bps,
        quorum: config.quorum,
        operator_count: OPERATOR_COUNT.load(deps.storage)?,
    })
}

pub fn operators(deps: Deps) -> Result<OperatorsResponse, ContractError> {
    Ok(OperatorsResponse {
        operators: OPERATORS
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()?,
    })
}

pub fn bounds(deps: Deps, domain: u32) -> Result<Option<Bounds>, ContractError> {
    Ok(BOUNDS.may_load(deps.storage, domain)?)
}

pub fn current_epoch(deps: Deps, env: Env) -> Result<EpochResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let now = env.block.time.seconds();
    let epoch = now / config.epoch_duration_secs;
    Ok(EpochResponse {
        epoch,
        starts_at: epoch * config.epoch_duration_secs,
        ends_at: (epoch + 1) * config.epoch_duration_secs,
    })
}

pub fn submissions(deps: Deps, domain: u32, epoch: u64) -> Result<SubmissionsResponse, ContractError> {
    let submissions = SUBMISSIONS
        .prefix((domain, epoch))
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            item.map(|(operator, submission)| SubmissionEntry {
                operator,
                submission,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(SubmissionsResponse { submissions })
}

pub fn applied(deps: Deps, domain: u32, epoch: u64) -> Result<Option<AppliedGasData>, ContractError> {
    Ok(APPLIED.may_load(deps.storage, (domain, epoch))?)
}

pub fn last_applied(deps: Deps, domain: u32) -> Result<Option<AppliedGasData>, ContractError> {
    Ok(LAST_APPLIED.may_load(deps.storage, domain)?)
}
