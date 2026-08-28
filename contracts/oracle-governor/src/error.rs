use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("unauthorized: sender is not the owner")]
    Unauthorized {},

    #[error("unauthorized: sender is not a registered operator")]
    NotOperator {},

    #[error("no bounds set for domain {domain} — governance must SetBounds first")]
    NoBounds { domain: u32 },

    #[error("invalid bounds: min must be <= max and max > 0")]
    InvalidBounds {},

    #[error("{field} {value} out of bounds [{min}, {max}] for domain {domain}")]
    OutOfBounds {
        field: String,
        value: String,
        min: String,
        max: String,
        domain: u32,
    },

    #[error("epoch {epoch} for domain {domain} already applied — submit on the next epoch")]
    EpochAlreadyApplied { domain: u32, epoch: u64 },

    // The median jumped more than the per-epoch limit. Nothing is applied; governance
    // resolves it via the emergency path (ForceSetRemoteGasData) or by adjusting the bounds.
    #[error("{field} delta too large for domain {domain}: last {last}, median {median}, limit {max_delta_bps} bps")]
    DeltaExceeded {
        field: String,
        domain: u32,
        last: String,
        median: String,
        max_delta_bps: u64,
    },

    #[error("quorum must be >= 1 and <= number of operators ({operators})")]
    InvalidQuorum { operators: u32 },

    #[error("epoch duration must be greater than zero")]
    ZeroEpoch {},

    #[error("operator list would become empty")]
    NoOperators {},
}
