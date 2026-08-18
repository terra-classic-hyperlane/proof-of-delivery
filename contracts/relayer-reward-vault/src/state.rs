use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// Governança on-chain do Terra Classic.
    pub owner: Addr,
    /// hpl-mailbox (fonte da prova de entrega, via raw query).
    pub mailbox: Addr,
    /// hpl-igp do qual este vault é beneficiary.
    pub igp: Addr,
    /// Denom do pool ("uluna").
    pub denom: String,
    /// Tarifa fixa por entrega comprovada.
    pub reward_per_delivery: Uint128,
    /// Janela de resgate em blocos a partir do bloco da entrega.
    pub claim_window_blocks: u64,
    pub paused: bool,
}

#[cw_serde]
pub struct ClaimRecord {
    pub claimant: Addr,
    pub amount: Uint128,
    pub claimed_at_block: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");

/// message_id (32 bytes) → registro do pagamento. A existência da chave é o que
/// impede resgate duplo; gravamos ANTES do BankMsg (padrão effects-first).
pub const CLAIMED: Map<Vec<u8>, ClaimRecord> = Map::new("claimed");

/// Métricas agregadas (auditoria barata sem varrer o Map).
pub const TOTAL_PAID: Item<Uint128> = Item::new("total_paid");
pub const TOTAL_CLAIMS: Item<u64> = Item::new("total_claims");
