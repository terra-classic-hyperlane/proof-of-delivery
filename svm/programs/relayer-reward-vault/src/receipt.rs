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
    program::invoke_signed,
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_program,
    sysvar::{instructions as sysvar_instructions, Sysvar},
};

use crate::{create_pda, custom, ensure, load_streaming, SEED_PREFIX, SEED_SEP};

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
        let index = u32::from_be_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);
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

// ===========================================================================
// PAPEL DESTINO — send_receipt_atomic (na MESMA tx da entrega TC→Solana)
// ===========================================================================
// Prova, por INTROSPECTION, que a instrução IRMÃ `InboxProcess` do Mailbox
// entregou a mensagem (executor = conta 0 dela) e despacha o recibo de volta.
//
// Contas:
//  0 keeper (signer, payer)
//  1 instructions sysvar
//  2 receipted PDA (w) — anti-duplo por message_id (keeper paga o rent)
//  3 system program
//  4 router PDA do `origin` (ro) — dá o recipient (vault de origem, 32B)
//  5 operator_of_local PDA do executor (ro) — dá o índice do operador
//  --- contas do OutboxDispatch (CPI no Mailbox) ---
//  6 mailbox program
//  7 outbox PDA (w)
//  8 dispatch authority PDA do pod (assina via invoke_signed)
//  9 spl_noop program
// 10 unique message account (signer, fresh)
// 11 dispatched message PDA (w)
pub fn send_receipt_atomic(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let iter = &mut accounts.iter();
    let keeper = next_account_info(iter)?;
    ensure(keeper.is_signer, ProgramError::MissingRequiredSignature)?;
    let ix_sysvar = next_account_info(iter)?;
    ensure(*ix_sysvar.key == sysvar_instructions::id(), custom(ERR_NO_INBOX_PROCESS))?;

    // ---- introspection: acha a InboxProcess irmã e extrai msg + executor ----
    let (message, executor) = find_inbox_process(ix_sysvar)?;
    let message_id = hashv(&[&message]).to_bytes();
    let origin = origin_of(&message)?;

    // anti-duplo: cria a PDA receipted[id] (falha se já existe = já enviado)
    let receipted_info = next_account_info(iter)?;
    let system_info_a = next_account_info(iter)?;
    let (exp_receipted, rc_bump) = remote_claimed_pda(program_id, &message_id);
    ensure(*receipted_info.key == exp_receipted, ProgramError::InvalidSeeds)?;
    ensure(receipted_info.data_is_empty(), custom(ERR_ALREADY_PAID))?;
    create_pda(
        keeper, receipted_info, system_info_a, program_id, 8,
        &[SEED_PREFIX, SEED_SEP, b"rclm", SEED_SEP, &message_id, &[rc_bump]],
    )?;

    // router do origin → recipient (o vault de origem, 32B)
    let router_info = next_account_info(iter)?;
    let (exp_router, _) = remote_router_pda(program_id, origin);
    ensure(*router_info.key == exp_router && router_info.owner == program_id, custom(ERR_NO_ROUTER))?;
    let router: Bytes32Val = load_streaming(router_info)?;

    // executor → índice do operador (reverse-lookup)
    let oploc_info = next_account_info(iter)?;
    let (exp_oploc, _) = operator_of_local_pda(program_id, &executor);
    ensure(*oploc_info.key == exp_oploc && oploc_info.owner == program_id, custom(ERR_UNKNOWN_EXECUTOR))?;
    let idx: U32Val = load_streaming(oploc_info)?;

    // corpo do recibo: message_id(32) + índice(4 BE) — igual EVM/CW
    let mut body = message_id.to_vec();
    body.extend_from_slice(&idx.value.to_be_bytes());

    // ---- CPI OutboxDispatch (recibo → origin) assinando com a dispatch authority ----
    let mailbox_prog = next_account_info(iter)?;
    ensure(*mailbox_prog.key == MAILBOX_PROGRAM, custom(ERR_BAD_MAILBOX))?;
    let outbox_info = next_account_info(iter)?;
    let dispatch_auth_info = next_account_info(iter)?;
    let spl_noop_info = next_account_info(iter)?;
    let unique_info = next_account_info(iter)?;
    let dispatched_info = next_account_info(iter)?;

    let (exp_auth, auth_bump) = dispatch_authority_pda(program_id);
    ensure(*dispatch_auth_info.key == exp_auth, ProgramError::InvalidSeeds)?;

    // dados: [4u8 = variante OutboxDispatch][borsh(OutboxDispatch)]
    #[derive(BorshSerialize)]
    struct OutboxDispatch {
        sender: Pubkey,
        destination_domain: u32,
        recipient: [u8; 32],
        message_body: Vec<u8>,
    }
    let dispatch = OutboxDispatch {
        sender: *program_id,
        destination_domain: origin,
        recipient: router.value,
        message_body: body,
    };
    let mut data = vec![4u8];
    data.extend_from_slice(&borsh::to_vec(&dispatch).map_err(|_| ProgramError::InvalidInstructionData)?);

    let metas = vec![
        AccountMeta::new(*outbox_info.key, false),
        AccountMeta::new_readonly(*dispatch_auth_info.key, true),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(*spl_noop_info.key, false),
        AccountMeta::new(*keeper.key, true),
        AccountMeta::new(*unique_info.key, true),
        AccountMeta::new(*dispatched_info.key, false),
    ];
    let ix = SolInstruction { program_id: MAILBOX_PROGRAM, accounts: metas, data };
    invoke_signed(
        &ix,
        &[
            outbox_info.clone(),
            dispatch_auth_info.clone(),
            system_info_a.clone(),
            spl_noop_info.clone(),
            keeper.clone(),
            unique_info.clone(),
            dispatched_info.clone(),
        ],
        &[&[b"hyperlane_dispatcher", b"-", b"dispatch_authority", &[auth_bump]]],
    )?;
    Ok(())
}

/// Percorre as instruções da tx e retorna (message, executor) da InboxProcess do
/// Mailbox real. Falha se não houver — garante que a entrega ocorreu NESTA tx.
fn find_inbox_process(ix_sysvar: &AccountInfo) -> Result<(Vec<u8>, Pubkey), ProgramError> {
    let mut i = 0usize;
    while let Ok(ix) = sysvar_instructions::load_instruction_at_checked(i, ix_sysvar) {
        // Mailbox + variante InboxProcess (=1 no enum borsh) — conta 0 = payer/executor
        if ix.program_id == MAILBOX_PROGRAM && ix.data.first() == Some(&1u8) && !ix.accounts.is_empty() {
            let message = parse_inbox_message(&ix.data)?;
            let executor = ix.accounts[0].pubkey;
            return Ok((message, executor));
        }
        i += 1;
    }
    Err(custom(ERR_NO_INBOX_PROCESS))
}

/// dados da InboxProcess: [1][metadata: len u32 LE + bytes][message: len u32 LE + bytes]
fn parse_inbox_message(data: &[u8]) -> Result<Vec<u8>, ProgramError> {
    ensure(data.len() >= 5, ProgramError::InvalidInstructionData)?;
    let mut o = 1usize;
    let mlen = u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
    o += 4 + mlen; // pula metadata
    ensure(data.len() >= o + 4, ProgramError::InvalidInstructionData)?;
    let msglen = u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
    o += 4;
    ensure(data.len() >= o + msglen, ProgramError::InvalidInstructionData)?;
    Ok(data[o..o + msglen].to_vec())
}

// ===========================================================================
// Admin (registry) — gated por operador (config)
// ===========================================================================
use crate::Config;

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
