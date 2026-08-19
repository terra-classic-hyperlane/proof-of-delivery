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

// ---------------------------------------------------------------------------
// Fase 1 (recibo trustless) — REGISTRO DE/PARA GLOBAL de operadores.
// Um operador é UMA identidade com um endereço por domínio. Este registro é a
// espinha dorsal: o recibo carrega o ÍNDICE do operador (u32), e cada chain
// resolve o endereço de pagamento no SEU próprio registro (definido pelo owner).
// ---------------------------------------------------------------------------

/// índice do operador → (domínio → endereço naquele domínio, como string).
/// Endereço no formato nativo da chain do domínio (terra1…/0x…/base58).
pub const OPERATOR_ADDR: Map<(u32, u32), String> = Map::new("op_addr");

/// reverse-lookup: endereço LOCAL (minúsculo p/ 0x…) → índice do operador.
/// Preenchido pelo owner ao gravar o endereço do operador NESTE domínio; é como
/// o papel DESTINO descobre "o executor processor(id) é o operador N".
pub const OPERATOR_OF_LOCAL: Map<String, u32> = Map::new("op_of_local");

/// maior índice de operador já registrado (+1 = próximo livre). Informativo.
pub const OPERATOR_COUNT: Item<u32> = Item::new("op_count");

/// router confiável por domínio: o endereço do NOSSO vault naquela chain
/// (hex de 32 bytes, convenção Hyperlane). Papel ORIGEM só aceita `handle` de um
/// router registrado; papel DESTINO despacha o recibo para ele.
pub const REMOTE_ROUTER: Map<u32, String> = Map::new("remote_router");
