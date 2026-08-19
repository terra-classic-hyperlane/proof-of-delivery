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

// ---------------------------------------------------------------------------
// v2 — ClaimRemote: pagamento da taxa de ORIGEM por entregas em chains remotas,
// via atestação com quórum (o TC não enxerga outras chains; a confiança fica
// no conjunto de atestadores + vínculos, ambos definidos pelo owner/governança).
// ---------------------------------------------------------------------------

#[cw_serde]
#[derive(Default)]
pub struct RemoteConfig {
    /// Operadores autorizados a atestar entregas remotas.
    pub attestors: Vec<Addr>,
    /// Atestações CONCORDANTES (mesmo executor) necessárias p/ pagar.
    pub quorum: u32,
}

#[cw_serde]
pub struct RemoteClaimRecord {
    pub executor: Addr,
    pub domain: u32,
    pub amount: Uint128,
    pub claimed_at_block: u64,
}

pub const REMOTE_CONFIG: Item<RemoteConfig> = Item::new("remote_config");
/// (operador TC, domain) → endereço remoto vinculado (hex 0x… minúsculo ou base58).
pub const REMOTE_BINDINGS: Map<(&Addr, u32), String> = Map::new("remote_bindings");
/// domain → recompensa fixa por entrega remota (0/ausente = domínio desativado).
pub const REMOTE_REWARDS: Map<u32, Uint128> = Map::new("remote_rewards");
/// message_id → pagamento remoto efetuado (existência = anti-duplo, effects-first).
pub const REMOTE_CLAIMED: Map<Vec<u8>, RemoteClaimRecord> = Map::new("remote_claimed");
/// message_id → atestações acumuladas (atestador, executor apontado).
pub const REMOTE_ATTESTS: Map<Vec<u8>, Vec<(Addr, Addr)>> = Map::new("remote_attests");
pub const TOTAL_REMOTE_PAID: Item<Uint128> = Item::new("total_remote_paid");
