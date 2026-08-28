use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::state::{AppliedGasData, Bounds, PriceSubmission};

#[cw_serde]
pub struct InstantiateMsg {
    /// On-chain governance of Terra Classic.
    pub owner: String,
    /// hpl-igp-oracle to govern.
    pub oracle: String,
    /// Initial operators (relayer addresses).
    pub operators: Vec<String>,
    /// Submissions required to apply (1 <= quorum <= operators).
    pub quorum: u32,
    /// Epoch duration in seconds (6h = 21_600).
    pub epoch_duration_secs: u64,
    /// Maximum variation per application, in bps (2000 = 20%).
    pub max_delta_bps: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // ---------------- operators (quorum) ----------------
    /// Submits the observed price for a domain in the current epoch. Upon reaching
    /// the quorum, the MEDIAN (lower of the central ones on an even tie — when in
    /// doubt, charge the user less) is validated against the bounds and the delta
    /// and applied to the oracle.
    SubmitPrice {
        domain: u32,
        token_exchange_rate: Uint128,
        gas_price: Uint128,
    },

    // ---------------- governance (owner) ----------------
    SetBounds { domain: u32, bounds: Bounds },
    UnsetBounds { domain: u32 },
    SetOperators {
        add: Vec<String>,
        remove: Vec<String>,
    },
    SetQuorum { quorum: u32 },
    SetEpochDuration { epoch_duration_secs: u64 },
    SetMaxDeltaBps { max_delta_bps: u64 },
    SetOwner { owner: String },

    /// EMERGENCY (spec §10): governance writes directly to the oracle, ignoring
    /// quorum, bounds and delta. Updates the delta base.
    ForceSetRemoteGasData {
        domain: u32,
        token_exchange_rate: Uint128,
        gas_price: Uint128,
    },

    /// EMERGENCY EXIT: starts returning ownership of the oracle (the recipient
    /// must call ClaimOwnership on the oracle). Owner-only.
    InitOracleOwnershipTransfer { next_owner: String },
    /// Cancels a started transfer. Owner-only.
    RevokeOracleOwnershipTransfer {},
    /// Claims ownership of the oracle when this contract is the pending owner
    /// (step 2 of the installation). Permissionless: only accepted if the oracle agrees.
    ClaimOracleOwnership {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(OperatorsResponse)]
    Operators {},

    #[returns(Option<Bounds>)]
    Bounds { domain: u32 },

    /// Current epoch derived from the block timestamp.
    #[returns(EpochResponse)]
    CurrentEpoch {},

    /// Submissions registered for (domain, epoch).
    #[returns(SubmissionsResponse)]
    Submissions { domain: u32, epoch: u64 },

    /// What was applied in (domain, epoch), if anything.
    #[returns(Option<AppliedGasData>)]
    Applied { domain: u32, epoch: u64 },

    /// Last value applied per domain (delta base).
    #[returns(Option<AppliedGasData>)]
    LastApplied { domain: u32 },
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub oracle: Addr,
    pub epoch_duration_secs: u64,
    pub max_delta_bps: u64,
    pub quorum: u32,
    pub operator_count: u32,
}

#[cw_serde]
pub struct OperatorsResponse {
    pub operators: Vec<Addr>,
}

#[cw_serde]
pub struct EpochResponse {
    pub epoch: u64,
    pub starts_at: u64,
    pub ends_at: u64,
}

#[cw_serde]
pub struct SubmissionEntry {
    pub operator: Addr,
    pub submission: PriceSubmission,
}

#[cw_serde]
pub struct SubmissionsResponse {
    pub submissions: Vec<SubmissionEntry>,
}
