use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, HexBinary, Uint128, Uint256};

#[cw_serde]
pub struct InstantiateMsg {
    /// On-chain governance of Terra Classic (gov module address) — never a multisig.
    pub owner: String,
    /// hpl-mailbox in production (source of the delivery proof).
    pub mailbox: String,
    /// hpl-igp of which this vault is the beneficiary (target of the Sweep).
    pub igp: String,
    /// Denom of the pool and of the rewards (Terra Classic: "uluna").
    pub denom: String,
    /// Fixed fee paid per proven delivery.
    pub reward_per_delivery: Uint128,
    /// Claim window in blocks, counted from the delivery block.
    pub claim_window_blocks: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Claims the reward for proven deliveries. ATOMIC: any invalid id
    /// (not delivered, from another relayer, expired, duplicated or already paid) reverts the batch.
    Claim { message_ids: Vec<HexBinary> },

    /// Permissionless: makes the vault pull the accumulated IGP balance
    /// (the IGP's `claim()` only accepts the beneficiary — this contract).
    Sweep {},

    /// Owner only (governance).
    UpdateConfig {
        owner: Option<String>,
        mailbox: Option<String>,
        igp: Option<String>,
        reward_per_delivery: Option<Uint128>,
        claim_window_blocks: Option<u64>,
    },

    /// Owner only (governance).
    SetPause { paused: bool },

    /// Owner only (governance): withdraws surplus from the pool.
    WithdrawSurplus { to: String, amount: Uint128 },

    // ---- v2 ClaimRemote: origin fee paid per attested REMOTE delivery ----
    /// Owner only: defines the attestors of remote deliveries and the quorum of
    /// agreeing attestations (with 1 operator the quorum is 1 = self-attestation;
    /// raise to >= 2 when there are independent operators).
    SetRemoteOperators { attestors: Vec<String>, quorum: u32 },

    /// Owner only: binds the operator's REMOTE address on a domain
    /// (`None` removes). It is the TC ↔ remote chain identity link.
    SetRemoteBinding {
        operator: String,
        domain: u32,
        remote_address: Option<String>,
    },

    /// Owner only: fixed reward per remote delivery on the domain (0 disables).
    SetRemoteReward { domain: u32, reward: Uint128 },

    // ---- Phase 1 (trustless receipt): global operator lookup registry ----
    /// Owner only: writes the address of operator `index` on `domain` (`None`
    /// removes). When `domain` = THIS domain, it also feeds the reverse-lookup
    /// (local executor → index) used by the DESTINATION role.
    SetOperatorAddress {
        index: u32,
        domain: u32,
        address: Option<String>,
    },

    /// Owner only: registers/updates the router (our vault) of a domain (`None`
    /// removes). `address` in the hex-32 format of the Hyperlane convention.
    SetRemoteRouter { domain: u32, address: Option<String> },

    // ---- Phase 2/3 (trustless receipt) ----
    /// DESTINATION ROLE. Proves that these MESSAGES (full bytes) were delivered
    /// HERE (raw query DELIVERIES by keccak256(msg)) and dispatches ONE receipt
    /// back to the origin vault — the origin domain is READ from the message (not
    /// forgeable). Attached funds pay the receipt's hook/IGP (the operator pays).
    /// `gas_limit`: gas charged by the IGP to DELIVER the receipt (via metadata).
    /// Without it the IGP uses gas_for_domain — which is the FULL user TARIFF
    /// ($0.08); the receipt should pay only the real gas (e.g.: 300k), otherwise the
    /// operator's commission is eaten by the receipt's own fee.
    SendReceipt { messages: Vec<HexBinary>, gas_limit: Option<Uint256> },

    /// ORIGIN ROLE. Called by the hpl-mailbox when delivering the receipt. Only accepts from
    /// the Mailbox and from a `sender` == the registered router of `origin`. Pays each id to
    /// the address of operator N in OUR local registry. Idempotent.
    Handle(HandleMsg),

    /// Attestor: asserts that the messages (dispatched FROM THIS mailbox to `domain`
    /// — the message_id is the SAME on both chains) were delivered there by the address
    /// bound to `executor` (default: the attestor itself). Upon reaching the
    /// quorum of AGREEING attestations it pays the reward — ONCE per id.
    AttestRemoteDelivery {
        domain: u32,
        message_ids: Vec<HexBinary>,
        executor: Option<String>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    /// Claim status of a message.
    #[returns(ClaimedResponse)]
    Claimed { message_id: HexBinary },

    /// Direct read (raw query) of the Mailbox's DELIVERIES for a message.
    #[returns(DeliveryResponse)]
    Delivery { message_id: HexBinary },

    /// Probes the Mailbox storage layout against a message KNOWN to be delivered.
    /// Monitor after any Mailbox migrate (spec §06).
    #[returns(LayoutCheckResponse)]
    LayoutCheck { message_id: HexBinary },

    #[returns(SolvencyResponse)]
    Solvency {},

    // ---- v2 ClaimRemote ----
    #[returns(RemoteConfigResponse)]
    RemoteConfig {},

    #[returns(RemoteBindingResponse)]
    RemoteBinding { operator: String, domain: u32 },

    #[returns(RemoteRewardResponse)]
    RemoteReward { domain: u32 },

    /// Remote payment status of a message.
    #[returns(RemoteClaimedResponse)]
    RemoteClaimed { message_id: HexBinary },

    /// Pending attestations of a message (public audit).
    #[returns(RemoteAttestationsResponse)]
    RemoteAttestations { message_id: HexBinary },

    /// How much these ids WOULD PAY if confirmed (not yet paid) — to decide whether
    /// it is worth the gas of sending the receipt. amount = payable_count × domain reward.
    #[returns(QuoteRemoteResponse)]
    QuoteRemote { domain: u32, message_ids: Vec<HexBinary> },

    // ---- Phase 1: lookup registry ----
    /// Address of operator `index` on `domain` (lookup registry).
    #[returns(OperatorAddressResponse)]
    OperatorAddress { index: u32, domain: u32 },

    /// Index of the operator owning a LOCAL address (reverse-lookup).
    #[returns(OperatorOfLocalResponse)]
    OperatorOfLocal { address: String },

    /// Router (our vault) registered for a domain.
    #[returns(RemoteRouterResponse)]
    RemoteRouter { domain: u32 },

    /// Hyperlane: the Mailbox asks the recipient's ISM when delivering (the receipt).
    /// Mirrors `hpl_interface::ism::IsmSpecifierQueryMsg` — we return `None`
    /// (uses the Mailbox's default ISM, which already validates the TC↔BSC corridor).
    #[returns(InterchainSecurityModuleResponse)]
    IsmSpecifier(IsmSpecifierQueryMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum IsmSpecifierQueryMsg {
    #[returns(InterchainSecurityModuleResponse)]
    InterchainSecurityModule(),
}

#[cw_serde]
pub struct InterchainSecurityModuleResponse {
    pub ism: Option<Addr>,
}

/// Mirrors `hpl_interface::core::HandleMsg` (what the Mailbox sends to the recipient).
#[cw_serde]
pub struct HandleMsg {
    pub origin: u32,
    pub sender: HexBinary,
    pub body: HexBinary,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub mailbox: Addr,
    pub igp: Addr,
    pub denom: String,
    pub reward_per_delivery: Uint128,
    pub claim_window_blocks: u64,
    pub paused: bool,
    pub total_paid: Uint128,
    pub total_claims: u64,
}

#[cw_serde]
pub struct ClaimedResponse {
    pub claimed: bool,
    pub claimant: Option<Addr>,
    pub amount: Option<Uint128>,
    pub claimed_at_block: Option<u64>,
}

#[cw_serde]
pub struct DeliveryResponse {
    pub delivered: bool,
    /// Who executed process() — the economic owner of the delivery.
    pub processor: Option<Addr>,
    pub delivered_at_block: Option<u64>,
}

#[cw_serde]
pub struct LayoutCheckResponse {
    /// true = the key exists and the value parses strictly as `Delivery`.
    pub ok: bool,
    pub detail: String,
}

#[cw_serde]
pub struct SolvencyResponse {
    pub pool: Coin,
    pub reward_per_delivery: Uint128,
    /// How many deliveries the current pool can pay.
    pub claims_payable: Uint128,
}

// ---- v2 ClaimRemote ----
#[cw_serde]
pub struct RemoteConfigResponse {
    pub attestors: Vec<Addr>,
    pub quorum: u32,
    pub total_remote_paid: Uint128,
}

#[cw_serde]
pub struct RemoteBindingResponse {
    pub remote_address: Option<String>,
}

#[cw_serde]
pub struct RemoteRewardResponse {
    pub reward: Option<Uint128>,
}

#[cw_serde]
pub struct RemoteClaimedResponse {
    pub claimed: bool,
    pub executor: Option<Addr>,
    pub domain: Option<u32>,
    pub amount: Option<Uint128>,
    pub claimed_at_block: Option<u64>,
}

#[cw_serde]
pub struct RemoteAttestationsResponse {
    /// (attestor, pointed executor)
    pub attestations: Vec<(Addr, Addr)>,
}

#[cw_serde]
pub struct QuoteRemoteResponse {
    pub amount: Uint128,
    pub payable_count: u32,
}

// ---- Phase 1: lookup registry ----
#[cw_serde]
pub struct OperatorAddressResponse {
    pub address: Option<String>,
}

#[cw_serde]
pub struct OperatorOfLocalResponse {
    pub index: Option<u32>,
}

#[cw_serde]
pub struct RemoteRouterResponse {
    pub address: Option<String>,
}
