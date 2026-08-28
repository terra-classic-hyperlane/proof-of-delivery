use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// Terra Classic on-chain governance.
    pub owner: Addr,
    /// hpl-mailbox (source of the delivery proof, via raw query).
    pub mailbox: Addr,
    /// hpl-igp of which this vault is beneficiary.
    pub igp: Addr,
    /// Pool denom ("uluna").
    pub denom: String,
    /// Fixed reward per proven delivery.
    pub reward_per_delivery: Uint128,
    /// Claim window in blocks starting from the delivery block.
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

/// message_id (32 bytes) → payment record. The existence of the key is what
/// prevents double claim; we write it BEFORE the BankMsg (effects-first pattern).
pub const CLAIMED: Map<Vec<u8>, ClaimRecord> = Map::new("claimed");

/// Aggregate metrics (cheap auditing without scanning the Map).
pub const TOTAL_PAID: Item<Uint128> = Item::new("total_paid");
pub const TOTAL_CLAIMS: Item<u64> = Item::new("total_claims");

// ---------------------------------------------------------------------------
// v2 — ClaimRemote: payment of the ORIGIN fee for deliveries on remote chains,
// via attestation with quorum (the TC does not see other chains; the trust rests
// on the set of attestors + bindings, both defined by the owner/governance).
// ---------------------------------------------------------------------------

#[cw_serde]
#[derive(Default)]
pub struct RemoteConfig {
    /// Operators authorized to attest remote deliveries.
    pub attestors: Vec<Addr>,
    /// AGREEING attestations (same executor) required to pay.
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
/// (TC operator, domain) → bound remote address (hex 0x… lowercase or base58).
pub const REMOTE_BINDINGS: Map<(&Addr, u32), String> = Map::new("remote_bindings");
/// domain → fixed reward per remote delivery (0/absent = domain disabled).
pub const REMOTE_REWARDS: Map<u32, Uint128> = Map::new("remote_rewards");
/// message_id → remote payment made (existence = anti-double, effects-first).
pub const REMOTE_CLAIMED: Map<Vec<u8>, RemoteClaimRecord> = Map::new("remote_claimed");
/// message_id → accumulated attestations (attestor, pointed executor).
pub const REMOTE_ATTESTS: Map<Vec<u8>, Vec<(Addr, Addr)>> = Map::new("remote_attests");
pub const TOTAL_REMOTE_PAID: Item<Uint128> = Item::new("total_remote_paid");
/// message_id → block in which the receipt (DESTINATION role) was dispatched.
/// Existence = already issued a receipt for this id → does NOT reissue. The payment
/// idempotency lives HERE (on the issuing destination), because destinations like Solana
/// cannot deduplicate in `handle` (the Mailbox does not pass a payer to create an account).
pub const SENT_RECEIPT: Map<Vec<u8>, u64> = Map::new("sent_receipt");

// ---------------------------------------------------------------------------
// Phase 1 (trustless receipt) — GLOBAL FROM/TO REGISTRY of operators.
// An operator is ONE identity with one address per domain. This registry is the
// backbone: the receipt carries the operator INDEX (u32), and each chain
// resolves the payment address in ITS own registry (defined by the owner).
// ---------------------------------------------------------------------------

/// operator index → (domain → address in that domain, as string).
/// Address in the native format of the domain's chain (terra1…/0x…/base58).
pub const OPERATOR_ADDR: Map<(u32, u32), String> = Map::new("op_addr");

/// reverse-lookup: LOCAL address (lowercase for 0x…) → operator index.
/// Filled by the owner when writing the operator's address in THIS domain; it is how
/// the DESTINATION role finds out "the executor processor(id) is operator N".
pub const OPERATOR_OF_LOCAL: Map<String, u32> = Map::new("op_of_local");

/// highest operator index already registered (+1 = next free). Informational.
pub const OPERATOR_COUNT: Item<u32> = Item::new("op_count");

/// trusted router per domain: the address of OUR vault on that chain
/// (32-byte hex, Hyperlane convention). The ORIGIN role only accepts `handle` from a
/// registered router; the DESTINATION role dispatches the receipt to it.
pub const REMOTE_ROUTER: Map<u32, String> = Map::new("remote_router");
