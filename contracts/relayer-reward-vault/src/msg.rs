use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, HexBinary, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    /// Governança on-chain do Terra Classic (endereço do módulo gov) — nunca multisig.
    pub owner: String,
    /// hpl-mailbox em produção (fonte da prova de entrega).
    pub mailbox: String,
    /// hpl-igp do qual este vault é beneficiary (alvo do Sweep).
    pub igp: String,
    /// Denom do pool e das recompensas (Terra Classic: "uluna").
    pub denom: String,
    /// Tarifa fixa paga por entrega comprovada.
    pub reward_per_delivery: Uint128,
    /// Janela de resgate em blocos, contada do bloco da entrega.
    pub claim_window_blocks: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Resgata a recompensa das entregas comprovadas. ATÔMICO: qualquer id inválido
    /// (não entregue, de outro relayer, expirado, duplicado ou já pago) reverte o lote.
    Claim { message_ids: Vec<HexBinary> },

    /// Permissionless: manda o vault puxar o saldo acumulado do IGP
    /// (o `claim()` do IGP só aceita o beneficiary — este contrato).
    Sweep {},

    /// Só o owner (governança).
    UpdateConfig {
        owner: Option<String>,
        mailbox: Option<String>,
        igp: Option<String>,
        reward_per_delivery: Option<Uint128>,
        claim_window_blocks: Option<u64>,
    },

    /// Só o owner (governança).
    SetPause { paused: bool },

    /// Só o owner (governança): retira excedente do pool.
    WithdrawSurplus { to: String, amount: Uint128 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    /// Status de resgate de uma mensagem.
    #[returns(ClaimedResponse)]
    Claimed { message_id: HexBinary },

    /// Leitura direta (raw query) do DELIVERIES do Mailbox para uma mensagem.
    #[returns(DeliveryResponse)]
    Delivery { message_id: HexBinary },

    /// Sonda o layout do storage do Mailbox contra uma mensagem SABIDAMENTE entregue.
    /// Monitorar após qualquer migrate do Mailbox (spec §06).
    #[returns(LayoutCheckResponse)]
    LayoutCheck { message_id: HexBinary },

    #[returns(SolvencyResponse)]
    Solvency {},
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
    /// Quem executou o process() — o dono econômico da entrega.
    pub processor: Option<Addr>,
    pub delivered_at_block: Option<u64>,
}

#[cw_serde]
pub struct LayoutCheckResponse {
    /// true = a chave existe e o valor parseia estritamente como `Delivery`.
    pub ok: bool,
    pub detail: String,
}

#[cw_serde]
pub struct SolvencyResponse {
    pub pool: Coin,
    pub reward_per_delivery: Uint128,
    /// Quantas entregas o pool atual consegue pagar.
    pub claims_payable: Uint128,
}
