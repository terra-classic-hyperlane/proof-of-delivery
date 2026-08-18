//! Espelho mínimo da interface do hpl-igp-oracle (tc-cw-hyperlane,
//! `packages/interface/src/igp/oracle.rs` e `ownable.rs`). Duplicado aqui de
//! propósito: os nomes serde precisam bater byte a byte com o contrato em
//! produção, e importar o workspace inteiro do hpl só para 4 tipos não vale.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct RemoteGasDataConfig {
    pub remote_domain: u32,
    pub token_exchange_rate: Uint128,
    pub gas_price: Uint128,
}

/// hpl_interface::ownable::OwnableMsg — posse em DOIS passos.
#[cw_serde]
pub enum OwnableMsg {
    InitOwnershipTransfer { next_owner: String },
    RevokeOwnershipTransfer {},
    ClaimOwnership {},
}

/// hpl_interface::igp::oracle::ExecuteMsg (subset usado pelo governor).
#[cw_serde]
pub enum OracleExecuteMsg {
    Ownership(OwnableMsg),
    SetRemoteGasDataConfigs { configs: Vec<RemoteGasDataConfig> },
    SetRemoteGasData { config: RemoteGasDataConfig },
}
