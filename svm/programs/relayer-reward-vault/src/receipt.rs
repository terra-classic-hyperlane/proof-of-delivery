#![allow(unused_imports)] // send_receipt_atomic (introspection+CPI) usa estes; a ser adicionado
//! Recibo trustless na Solana (espelho do TC↔BSC).
//!
//! DOIS papéis, como nos vaults EVM/CW:
//!  - ORIGEM (`handle`): o Mailbox entrega um recibo (mensagem de outra chain que
//!    PROVOU a entrega); pagamos o operador em SOL. Sem confiança: o recibo veio
//!    validado pelos validadores e do router registrado.
//!  - DESTINO (`send_receipt_atomic`): na MESMA tx da entrega TC→Solana, lemos a
//!    instrução `InboxProcess` irmã por INTROSPECTION — dela tiramos a mensagem
//!    (→ id/origem) e o executor (conta 0, que assinou). Provado on-chain, sem
//!    confiar no chamador. Despacha o recibo de volta pelo Mailbox.
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    hash::hashv,
    instruction::{AccountMeta, Instruction as SolInstruction},
    program::{invoke, invoke_signed},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::{instructions as sysvar_instructions, Sysvar},
};

use crate::{custom, ensure, load_streaming, store, create_pda, SEED_PREFIX, SEED_SEP};

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
pub const ERR_NO_INBOX_PROCESS: u32 = 204;
pub const ERR_NOT_DELIVERED: u32 = 205;
pub const ERR_UNKNOWN_EXECUTOR: u32 = 206;
pub const ERR_ALREADY_PAID: u32 = 207;
pub const ERR_BAD_MAILBOX: u32 = 208;

// ---- PDAs ----
/// operador (índice) → pubkey de pagamento na Solana.
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
/// router confiável (nosso vault) por domínio — 32 bytes (convenção Hyperlane).
pub fn remote_router_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rrout", SEED_SEP, &domain.to_le_bytes()],
        program_id,
    )
}
/// message_id → pagamento remoto efetuado (anti-duplo).
pub fn remote_claimed_pda(program_id: &Pubkey, message_id: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rclm", SEED_SEP, message_id],
        program_id,
    )
}
/// dispatch authority (no MAILBOX) que assina o dispatch do recibo em nome do pod.
pub fn dispatch_authority_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    // seeds do pod: ["hyperlane_dispatcher","-","dispatch_authority"]
    Pubkey::find_program_address(
        &[b"hyperlane_dispatcher", b"-", b"dispatch_authority"],
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
#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct RemoteClaimRecord {
    pub operator_index: u32,
    pub origin_domain: u32,
    pub amount: u64,
    pub slot: u64,
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
// PAPEL ORIGEM — handle (o Mailbox entrega o recibo; pagamos SOL)
// ===========================================================================
// Contas (após o process_authority):
//  0 process_authority (signer, PDA do Mailbox p/ este recipient)
//  1 config (w) — o pool
//  2 router PDA do `origin` (ro) — confere sender == router
//  3 reward PDA do `origin` (ro) — lamports por entrega
//  4.. para cada (id,index) no corpo: operator_sol PDA (ro) + conta de pagamento (w)
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
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;

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
        let index = u32::from_le_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);
        // resolve o pubkey de pagamento do operador N
        let opsol_info = next_account_info(iter)?;
        let (exp_opsol, _) = operator_sol_pda(program_id, index);
        ensure(*opsol_info.key == exp_opsol && opsol_info.owner == program_id, custom(ERR_UNKNOWN_EXECUTOR))?;
        let payout: PubkeyVal = load_streaming(opsol_info)?;
        let payee_info = next_account_info(iter)?;
        ensure(*payee_info.key == payout.value, custom(ERR_UNKNOWN_EXECUTOR))?;
        // paga do pool (respeita rent-exempt do config)
        if reward == 0 { continue; }
        let pool_avail = config_info.lamports().saturating_sub(rent_floor);
        if reward > pool_avail { continue; } // pool sem fundo — pula (semear)
        **config_info.try_borrow_mut_lamports()? -= reward;
        **payee_info.try_borrow_mut_lamports()? += reward;
    }
    Ok(())
}

/// Responde a query InterchainSecurityModule do Mailbox (retorna o ISM do warp).
pub fn ism_response() -> ProgramResult {
    solana_program::program::set_return_data(WARP_ISM.as_ref());
    Ok(())
}
/// HandleAccountMetas / IsmAccountMetas: retornamos vazio aqui e exigimos que o
/// keeper/relayer monte as contas na ordem de `handle` (documentado). Para o
/// caminho de produção com o relayer padrão, o account-metas seria derivado do
/// corpo; nesta fase o keeper monta.
pub fn empty_metas() -> ProgramResult {
    // Vec<SerializableAccountMeta> vazio = 4 bytes de length 0
    solana_program::program::set_return_data(&0u32.to_le_bytes());
    Ok(())
}
