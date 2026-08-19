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

    // ---- v2 ClaimRemote: taxa de origem paga por entrega REMOTA atestada ----
    /// Só o owner: define os atestadores de entregas remotas e o quórum de
    /// atestações concordantes (com 1 operador o quórum é 1 = auto-atestação;
    /// subir p/ >= 2 quando houver operadores independentes).
    SetRemoteOperators { attestors: Vec<String>, quorum: u32 },

    /// Só o owner: vincula o endereço REMOTO do operador num domínio
    /// (`None` remove). É o elo de identidade TC ↔ chain remota.
    SetRemoteBinding {
        operator: String,
        domain: u32,
        remote_address: Option<String>,
    },

    /// Só o owner: recompensa fixa por entrega remota no domínio (0 desativa).
    SetRemoteReward { domain: u32, reward: Uint128 },

    /// Atestador: afirma que as mensagens (despachadas DESTE mailbox p/ `domain`
    /// — o message_id é o MESMO nas duas chains) foram entregues lá pelo endereço
    /// vinculado ao `executor` (default: o próprio atestador). Ao atingir o
    /// quórum de atestações CONCORDANTES paga a recompensa — UMA vez por id.
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

    // ---- v2 ClaimRemote ----
    #[returns(RemoteConfigResponse)]
    RemoteConfig {},

    #[returns(RemoteBindingResponse)]
    RemoteBinding { operator: String, domain: u32 },

    #[returns(RemoteRewardResponse)]
    RemoteReward { domain: u32 },

    /// Status do pagamento remoto de uma mensagem.
    #[returns(RemoteClaimedResponse)]
    RemoteClaimed { message_id: HexBinary },

    /// Atestações pendentes de uma mensagem (auditoria pública).
    #[returns(RemoteAttestationsResponse)]
    RemoteAttestations { message_id: HexBinary },

    /// Quanto estes ids PAGARIAM se confirmados (ainda não pagos) — decidir se
    /// vale o gás de enviar o recibo. amount = payable_count × recompensa do domínio.
    #[returns(QuoteRemoteResponse)]
    QuoteRemote { domain: u32, message_ids: Vec<HexBinary> },
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
    /// (atestador, executor apontado)
    pub attestations: Vec<(Addr, Addr)>,
}

#[cw_serde]
pub struct QuoteRemoteResponse {
    pub amount: Uint128,
    pub payable_count: u32,
}
