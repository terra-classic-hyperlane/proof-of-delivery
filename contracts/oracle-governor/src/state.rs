use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// Governança on-chain do Terra Classic. Define faixa, operadores e quórum —
    /// nunca os próprios operadores (é a trava do conflito de interesse, spec §10).
    pub owner: Addr,
    /// hpl-igp-oracle do qual este contrato é (ou será) o owner.
    pub oracle: Addr,
    /// Duração da época em segundos (sugestão da spec: 6h = 21_600).
    pub epoch_duration_secs: u64,
    /// Variação máxima por aplicação, em bps sobre o último valor aplicado
    /// (sugestão da spec: 2000 = 20%). Vale para os dois campos.
    pub max_delta_bps: u64,
    /// Quantas submissões idênticas de época são necessárias para aplicar.
    pub quorum: u32,
}

/// Faixa [min, max] por domínio remoto — definida pela governança. Sem faixa
/// cadastrada, NENHUMA submissão para o domínio é aceita.
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

/// O que foi aplicado no oracle para (domínio, época) + valores correntes.
#[cw_serde]
pub struct AppliedGasData {
    pub token_exchange_rate: Uint128,
    pub gas_price: Uint128,
    pub epoch: u64,
    /// true quando veio do caminho de emergência (ForceSetRemoteGasData).
    pub forced: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");
/// domínio → faixa vigente (só a governança escreve).
pub const BOUNDS: Map<u32, Bounds> = Map::new("bounds");
pub const OPERATORS: Map<&Addr, ()> = Map::new("operators");
pub const OPERATOR_COUNT: Item<u32> = Item::new("operator_count");

/// (domínio, época, operador) → submissão. O operador pode sobrescrever a própria
/// submissão enquanto a época não foi aplicada.
pub const SUBMISSIONS: Map<(u32, u64, &Addr), PriceSubmission> = Map::new("submissions");

/// (domínio, época) → aplicado? Uma aplicação por época por domínio.
pub const APPLIED: Map<(u32, u64), AppliedGasData> = Map::new("applied");

/// domínio → último valor efetivamente aplicado (base do delta de variação).
pub const LAST_APPLIED: Map<u32, AppliedGasData> = Map::new("last_applied");
