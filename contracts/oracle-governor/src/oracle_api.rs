//! Minimal mirror of the hpl-igp-oracle interface (tc-cw-hyperlane,
//! `packages/interface/src/igp/oracle.rs` and `ownable.rs`). Duplicated here on
//! purpose: the serde names must match byte for byte with the contract in
//! production, and importing the whole hpl workspace just for 4 types is not worth it.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct RemoteGasDataConfig {
    pub remote_domain: u32,
    pub token_exchange_rate: Uint128,
    pub gas_price: Uint128,
}

/// hpl_interface::ownable::OwnableMsg — ownership in TWO steps.
#[cw_serde]
pub enum OwnableMsg {
    InitOwnershipTransfer { next_owner: String },
    RevokeOwnershipTransfer {},
    ClaimOwnership {},
}

/// hpl_interface::igp::oracle::ExecuteMsg (subset used by the governor).
#[cw_serde]
pub enum OracleExecuteMsg {
    Ownership(OwnableMsg),
    SetRemoteGasDataConfigs { configs: Vec<RemoteGasDataConfig> },
    SetRemoteGasData { config: RemoteGasDataConfig },
}
