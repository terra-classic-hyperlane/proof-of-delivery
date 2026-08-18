use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::state::{AppliedGasData, Bounds, PriceSubmission};

#[cw_serde]
pub struct InstantiateMsg {
    /// Governança on-chain do Terra Classic.
    pub owner: String,
    /// hpl-igp-oracle a governar.
    pub oracle: String,
    /// Operadores iniciais (endereços dos relayers).
    pub operators: Vec<String>,
    /// Submissões necessárias para aplicar (1 <= quorum <= operadores).
    pub quorum: u32,
    /// Duração da época em segundos (6h = 21_600).
    pub epoch_duration_secs: u64,
    /// Variação máxima por aplicação, em bps (2000 = 20%).
    pub max_delta_bps: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // ---------------- operadores (quórum) ----------------
    /// Submete o preço observado para um domínio na época corrente. Ao atingir o
    /// quórum, a MEDIANA (menor dos centrais em empate par — na dúvida, cobra
    /// menos do usuário) é validada contra a faixa e o delta e aplicada no oracle.
    SubmitPrice {
        domain: u32,
        token_exchange_rate: Uint128,
        gas_price: Uint128,
    },

    // ---------------- governança (owner) ----------------
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

    /// EMERGÊNCIA (spec §10): a governança escreve direto no oracle, ignorando
    /// quórum, faixa e delta. Atualiza a base do delta.
    ForceSetRemoteGasData {
        domain: u32,
        token_exchange_rate: Uint128,
        gas_price: Uint128,
    },

    /// SAÍDA DE EMERGÊNCIA: inicia a devolução da posse do oracle (o destinatário
    /// precisa dar ClaimOwnership no oracle). Owner-only.
    InitOracleOwnershipTransfer { next_owner: String },
    /// Cancela uma transferência iniciada. Owner-only.
    RevokeOracleOwnershipTransfer {},
    /// Reivindica a posse do oracle quando este contrato é o pending owner
    /// (passo 2 da instalação). Permissionless: só aceita se o oracle concordar.
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

    /// Época corrente derivada do timestamp do bloco.
    #[returns(EpochResponse)]
    CurrentEpoch {},

    /// Submissões registradas para (domínio, época).
    #[returns(SubmissionsResponse)]
    Submissions { domain: u32, epoch: u64 },

    /// O que foi aplicado em (domínio, época), se algo foi.
    #[returns(Option<AppliedGasData>)]
    Applied { domain: u32, epoch: u64 },

    /// Último valor aplicado por domínio (base do delta).
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
