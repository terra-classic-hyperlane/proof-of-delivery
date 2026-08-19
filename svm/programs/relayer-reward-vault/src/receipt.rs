//! Recibo trustless na Solana — sentido **Solana→TC**, SEM keeper.
//!
//! O `pod` é um **recipient** Hyperlane que RECEBE o recibo (mensagem que o vault
//! do TC despachou de volta depois de provar a entrega Solana→TC) e paga SOL ao
//! operador. Quem entrega o recibo é o **relayer nativo**, sem alteração.
//!
//! Como o Mailbox nativo, ao chamar `handle`, NÃO passa um payer (só antepõe o
//! `process_authority`), o `handle`:
//!   - não pode criar contas (não há quem pague o rent) → o pagamento vai para a
//!     PDA `operator_sol(index)`, que é DERIVÁVEL só a partir da mensagem (o índice
//!     está no corpo do recibo). O operador saca depois via `withdraw_operator_sol`.
//!   - não deduplica por id → a idempotência mora no `send_receipt` do TC (o
//!     destino que emite o recibo). O Mailbox já garante entrega única por mensagem.
//!
//! Este programa NÃO toca no Mailbox, ISM, IGP ou warp nativos.
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

use crate::{create_pda, custom, ensure, load_streaming, Config, SEED_PREFIX, SEED_SEP};

// ---- endereços de produção (Solana mainnet) ----
/// Mailbox Sealevel em produção.
pub const MAILBOX_PROGRAM: Pubkey =
    solana_program::pubkey!("E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi");
/// ISM do warp sintético IGORFAKE (valida mensagens vindas do TC).
pub const WARP_ISM: Pubkey = solana_program::pubkey!("4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ");
/// Domínio local (Solana).
pub const SOLANA_DOMAIN: u32 = 1399811149;

// ---- discriminadores da interface MessageRecipient (hyperlane) ----
pub const HANDLE_DISC: [u8; 8] = [33, 210, 5, 66, 196, 212, 239, 142];
pub const ISM_DISC: [u8; 8] = [45, 18, 245, 87, 234, 46, 246, 15];
pub const ISM_METAS_DISC: [u8; 8] = [190, 214, 218, 129, 67, 97, 4, 76];
pub const HANDLE_METAS_DISC: [u8; 8] = [194, 141, 30, 82, 241, 41, 169, 52];

// ---- erros ----
pub const ERR_NOT_PROCESS_AUTH: u32 = 200;
pub const ERR_UNTRUSTED_ROUTER: u32 = 201;
pub const ERR_MALFORMED_RECEIPT: u32 = 202;
pub const ERR_NO_ROUTER: u32 = 203;
pub const ERR_UNKNOWN_EXECUTOR: u32 = 206;
pub const ERR_INSUFFICIENT_BALANCE: u32 = 209;

// ---- PDAs ----
/// operador (índice) → pubkey de pagamento na Solana; ACUMULA o SOL a sacar.
pub fn operator_sol_pda(program_id: &Pubkey, index: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"opsol", SEED_SEP, &index.to_le_bytes()],
        program_id,
    )
}
/// reverse-lookup: pubkey local (executor) → índice do operador.
pub fn operator_of_local_pda(program_id: &Pubkey, local: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"oploc", SEED_SEP, local.as_ref()],
        program_id,
    )
}
/// router confiável (vault do TC) por domínio — 32 bytes (convenção Hyperlane).
pub fn remote_router_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rrout", SEED_SEP, &domain.to_le_bytes()],
        program_id,
    )
}

// ---- state ----
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct U32Val {
    pub value: u32,
}
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct PubkeyVal {
    pub value: Pubkey,
}
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct Bytes32Val {
    pub value: [u8; 32],
}

// ---- formato de retorno da interface MessageRecipient (borsh) ----
// espelha `serializable_account_meta::{SerializableAccountMeta, SimulationReturnData}`
// do monorepo (mesmos campos, mesma ordem) — inlined p/ não puxar dependência.
#[derive(BorshSerialize)]
struct SerAccountMeta {
    pubkey: Pubkey,
    is_signer: bool,
    is_writable: bool,
}
#[derive(BorshSerialize)]
struct SimReturn {
    metas: Vec<SerAccountMeta>,
    /// workaround do truncamento de zeros à direita em return_data simulado.
    trailing: u8,
}
impl SimReturn {
    fn new(metas: Vec<SerAccountMeta>) -> Self {
        Self { metas, trailing: u8::MAX }
    }
    fn emit(self) -> ProgramResult {
        let data = borsh::to_vec(&self)?;
        solana_program::program::set_return_data(&data);
        Ok(())
    }
}

/// domínio de origem da msg Hyperlane: version(1)+nonce(4) → origin em [5..9].
pub fn origin_of(msg: &[u8]) -> Result<u32, ProgramError> {
    ensure(msg.len() >= 9, ProgramError::InvalidInstructionData)?;
    Ok(u32::from_be_bytes([msg[5], msg[6], msg[7], msg[8]]))
}

/// É uma instrução da interface MessageRecipient? (o Mailbox chama assim.)
pub fn recipient_discriminator(data: &[u8]) -> Option<[u8; 8]> {
    if data.len() >= 8 {
        let d: [u8; 8] = data[0..8].try_into().unwrap();
        if d == HANDLE_DISC || d == ISM_DISC || d == ISM_METAS_DISC || d == HANDLE_METAS_DISC {
            return Some(d);
        }
    }
    None
}

// ===========================================================================
// handle — o Mailbox nativo entrega o recibo; pagamos SOL (creditamos a PDA)
// ===========================================================================
// Contas (o Mailbox antepõe o process_authority; o resto vem do HandleAccountMetas):
//  0 process_authority (signer, PDA do Mailbox p/ este recipient)
//  1 config (w) — o pool
//  2 router PDA do `origin` (ro) — confere sender == router (vault do TC)
//  3 reward PDA do `origin` (ro) — lamports por entrega
//  4.. para cada (id,index) no corpo: operator_sol PDA(index) (w) — recebe o SOL
pub fn handle(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    origin: u32,
    sender: [u8; 32],
    body: &[u8],
) -> Result<(), ProgramError> {
    let iter = &mut accounts.iter();
    let process_auth = next_account_info(iter)?;
    // só o Mailbox (via sua process authority p/ este recipient) pode chamar
    let (expected_auth, _) = Pubkey::find_program_address(
        &[b"hyperlane", b"-", b"process_authority", b"-", program_id.as_ref()],
        &MAILBOX_PROGRAM,
    );
    ensure(process_auth.is_signer, custom(ERR_NOT_PROCESS_AUTH))?;
    ensure(*process_auth.key == expected_auth, custom(ERR_NOT_PROCESS_AUTH))?;

    let config_info = next_account_info(iter)?;
    let (exp_config, _) = crate::config_pda(program_id);
    ensure(
        *config_info.key == exp_config && config_info.owner == program_id,
        ProgramError::IncorrectProgramId,
    )?;

    let router_info = next_account_info(iter)?;
    let (exp_router, _) = remote_router_pda(program_id, origin);
    ensure(*router_info.key == exp_router && router_info.owner == program_id, custom(ERR_NO_ROUTER))?;
    let router: Bytes32Val = load_streaming(router_info)?;
    ensure(router.value == sender, custom(ERR_UNTRUSTED_ROUTER))?;

    let reward_info = next_account_info(iter)?;
    let (exp_reward, _) = crate::remote_reward_pda(program_id, origin);
    ensure(*reward_info.key == exp_reward && reward_info.owner == program_id, custom(ERR_NO_ROUTER))?;
    let reward: u64 = load_streaming(reward_info)?;

    ensure(!body.is_empty() && body.len() % 36 == 0, custom(ERR_MALFORMED_RECEIPT))?;
    let rent_floor = Rent::get()?.minimum_balance(config_info.data_len());
    for chunk in body.chunks(36) {
        let index = u32::from_be_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);
        // PDA do operador N (derivável do índice) — recebe o crédito em lamports
        let opsol_info = next_account_info(iter)?;
        let (exp_opsol, _) = operator_sol_pda(program_id, index);
        ensure(
            *opsol_info.key == exp_opsol && opsol_info.owner == program_id,
            custom(ERR_UNKNOWN_EXECUTOR),
        )?;
        if reward == 0 {
            continue;
        }
        let pool_avail = config_info.lamports().saturating_sub(rent_floor);
        if reward > pool_avail {
            continue; // pool sem fundo — pula (semear o pool)
        }
        **config_info.try_borrow_mut_lamports()? -= reward;
        **opsol_info.try_borrow_mut_lamports()? += reward;
    }
    Ok(())
}

/// Responde a query InterchainSecurityModule do Mailbox.
/// O Mailbox lê como `Option::<Pubkey>::try_from_slice` (processor.rs) → devolvemos
/// `Some(WARP_ISM)` em borsh (33 bytes: [1] + pubkey).
pub fn ism_response() -> ProgramResult {
    let data = borsh::to_vec(&Some(WARP_ISM))?;
    solana_program::program::set_return_data(&data);
    Ok(())
}

/// IsmAccountMetas: nosso ISM é constante (não lê conta) → vec vazio.
pub fn ism_account_metas() -> ProgramResult {
    SimReturn::new(vec![]).emit()
}

/// HandleAccountMetas: as contas que `handle` usa (após o process_authority que o
/// Mailbox antepõe), TODAS derivadas só da mensagem (por isso o pagamento vai p/ a
/// PDA operator_sol(index), e não p/ uma carteira externa que o relayer não teria
/// como descobrir ao simular).
pub fn handle_account_metas(program_id: &Pubkey, origin: u32, body: &[u8]) -> ProgramResult {
    ensure(!body.is_empty() && body.len() % 36 == 0, custom(ERR_MALFORMED_RECEIPT))?;
    let (config, _) = crate::config_pda(program_id);
    let (router, _) = remote_router_pda(program_id, origin);
    let (reward, _) = crate::remote_reward_pda(program_id, origin);
    let mut metas = vec![
        SerAccountMeta { pubkey: config, is_signer: false, is_writable: true },
        SerAccountMeta { pubkey: router, is_signer: false, is_writable: false },
        SerAccountMeta { pubkey: reward, is_signer: false, is_writable: false },
    ];
    for chunk in body.chunks(36) {
        let index = u32::from_be_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);
        let (opsol, _) = operator_sol_pda(program_id, index);
        metas.push(SerAccountMeta { pubkey: opsol, is_signer: false, is_writable: true });
    }
    SimReturn::new(metas).emit()
}

// ===========================================================================
// withdraw_operator_sol — o operador saca o SOL acumulado na sua PDA
// ===========================================================================
/// [signer(payout) w, opsol PDA(index) w] — o signer TEM de ser o pubkey registrado.
pub fn withdraw_operator_sol(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    index: u32,
    amount: u64,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let signer = next_account_info(iter)?;
    ensure(signer.is_signer, ProgramError::MissingRequiredSignature)?;
    let opsol_info = next_account_info(iter)?;
    let (exp, _) = operator_sol_pda(program_id, index);
    ensure(
        *opsol_info.key == exp && opsol_info.owner == program_id,
        custom(ERR_UNKNOWN_EXECUTOR),
    )?;
    let payout: PubkeyVal = load_streaming(opsol_info)?;
    ensure(payout.value == *signer.key, custom(ERR_UNKNOWN_EXECUTOR))?;
    // só o excedente ao rent-exempt da própria PDA pode sair
    let rent_floor = Rent::get()?.minimum_balance(opsol_info.data_len());
    let avail = opsol_info.lamports().saturating_sub(rent_floor);
    ensure(amount > 0 && amount <= avail, custom(ERR_INSUFFICIENT_BALANCE))?;
    **opsol_info.try_borrow_mut_lamports()? -= amount;
    **signer.try_borrow_mut_lamports()? += amount;
    Ok(())
}

// ===========================================================================
// Admin (registry) — gated por operador (config)
// ===========================================================================
fn require_operator(config_info: &AccountInfo, signer: &AccountInfo, program_id: &Pubkey) -> ProgramResult {
    ensure(signer.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(config.operators.iter().any(|o| o == signer.key), custom(crate::ERR_NOT_OPERATOR))?;
    Ok(())
}

/// [operator signer w(payer), config, router PDA w, system]
pub fn set_remote_router(program_id: &Pubkey, accounts: &[AccountInfo], domain: u32, router: [u8; 32]) -> ProgramResult {
    let iter = &mut accounts.iter();
    let signer = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    require_operator(config_info, signer, program_id)?;
    let router_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;
    let (exp, bump) = remote_router_pda(program_id, domain);
    ensure(*router_info.key == exp, ProgramError::InvalidSeeds)?;
    if router_info.data_is_empty() {
        create_pda(signer, router_info, system, program_id, 32,
            &[SEED_PREFIX, SEED_SEP, b"rrout", SEED_SEP, &domain.to_le_bytes(), &[bump]])?;
    }
    crate::store(router_info, &Bytes32Val { value: router })
}

/// [operator signer w(payer), config, opsol PDA w, oploc PDA w, system]
pub fn set_operator_sol(program_id: &Pubkey, accounts: &[AccountInfo], index: u32, operator: Pubkey) -> ProgramResult {
    let iter = &mut accounts.iter();
    let signer = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    require_operator(config_info, signer, program_id)?;
    let opsol_info = next_account_info(iter)?;
    let oploc_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;
    let (exp_sol, sol_bump) = operator_sol_pda(program_id, index);
    ensure(*opsol_info.key == exp_sol, ProgramError::InvalidSeeds)?;
    if opsol_info.data_is_empty() {
        create_pda(signer, opsol_info, system, program_id, 32,
            &[SEED_PREFIX, SEED_SEP, b"opsol", SEED_SEP, &index.to_le_bytes(), &[sol_bump]])?;
    }
    crate::store(opsol_info, &PubkeyVal { value: operator })?;
    let (exp_loc, loc_bump) = operator_of_local_pda(program_id, &operator);
    ensure(*oploc_info.key == exp_loc, ProgramError::InvalidSeeds)?;
    if oploc_info.data_is_empty() {
        create_pda(signer, oploc_info, system, program_id, 4,
            &[SEED_PREFIX, SEED_SEP, b"oploc", SEED_SEP, operator.as_ref(), &[loc_bump]])?;
    }
    crate::store(oploc_info, &U32Val { value: index })
}
