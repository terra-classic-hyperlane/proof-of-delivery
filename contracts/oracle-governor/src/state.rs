use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// On-chain governance of Terra Classic. Defines bounds, operators and quorum —
    /// never the operators themselves (this is the conflict-of-interest lock, spec §10).
    pub owner: Addr,
    /// hpl-igp-oracle of which this contract is (or will be) the owner.
    pub oracle: Addr,
    /// Epoch duration in seconds (spec suggestion: 6h = 21_600).
    pub epoch_duration_secs: u64,
    /// Maximum variation per application, in bps over the last applied value
    /// (spec suggestion: 2000 = 20%). Applies to both fields.
    pub max_delta_bps: u64,
    /// How many identical epoch submissions are required to apply.
    pub quorum: u32,
}

/// Bounds [min, max] per remote domain — defined by governance. Without
/// registered bounds, NO submission for the domain is accepted.
#[cw_serde]
pub struct Bounds {
    pub min_exchange_rate: Uint128,
    pub max_exchange_rate: Uint128,
    pub min_gas_price: Uint128,
    pub max_gas_price: Uint128,
}

#[cw_serde]
pub struct PriceSubmission {
    pub token_exchange_rate: Uint128,
    pub gas_price: Uint128,
}

/// What was applied to the oracle for (domain, epoch) + current values.
#[cw_serde]
pub struct AppliedGasData {
    pub token_exchange_rate: Uint128,
    pub gas_price: Uint128,
    pub epoch: u64,
    /// true when it came from the emergency path (ForceSetRemoteGasData).
    pub forced: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");
/// domain → current bounds (only governance writes).
pub const BOUNDS: Map<u32, Bounds> = Map::new("bounds");
pub const OPERATORS: Map<&Addr, ()> = Map::new("operators");
pub const OPERATOR_COUNT: Item<u32> = Item::new("operator_count");

/// (domain, epoch, operator) → submission. The operator can overwrite their own
/// submission while the epoch has not been applied.
pub const SUBMISSIONS: Map<(u32, u64, &Addr), PriceSubmission> = Map::new("submissions");

/// (domain, epoch) → applied? One application per epoch per domain.
pub const APPLIED: Map<(u32, u64), AppliedGasData> = Map::new("applied");

/// domain → last value effectively applied (base of the variation delta).
pub const LAST_APPLIED: Map<u32, AppliedGasData> = Map::new("last_applied");
