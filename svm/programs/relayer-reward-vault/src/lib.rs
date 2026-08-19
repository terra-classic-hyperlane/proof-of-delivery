//! # RelayerRewardVault (Solana / Sealevel) — spec §08/§09
//!
//! A PDA de config **é** o pool: registre-a como `beneficiary` do IGP e os
//! lamports do `Claim` do IGP se acumulam nela. Como o Mailbox de Solana NÃO
//! grava quem executou o `process()` (a `ProcessedMessage` não tem campo de
//! executor), a autoria vem de **quórum de operadores por época**:
//!
//! - Cada operador submete o MESMO relatório da época (lista de créditos
//!   ordenada por chave — regra de convergência da spec §09); o hash do borsh
//!   é comparado; quórum de hashes idênticos → créditos atribuídos.
//! - A janela de slots fica **imutável** após a primeira submissão.
//! - Divergência → época simplesmente não fecha (alarme é off-chain).
//! - Saque: débito direto de lamports do pool, respeitando o rent-exempt.
//! - Administração SEM admin único: `AdminEnvelope { nonce, action }`, com a
//!   PDA da proposta semeada pelo hash do envelope (todos convergem na mesma
//!   conta) e execução no quórum. Em `WithdrawSurplus` o destino faz parte do
//!   hash — aprova-se AQUELE destino.
//!
//! Este programa NÃO toca no Mailbox nem no IGP.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    hash::hashv,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::Sysvar,
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub mod receipt;

// ---------------------------------------------------------------------------
// Seeds (spec §09: seeds = ["rrv", "-", "prop", "-", hash(envelope)])
// ---------------------------------------------------------------------------
pub const SEED_PREFIX: &[u8] = b"rrv";
pub const SEED_SEP: &[u8] = b"-";
pub const SEED_CONFIG: &[u8] = b"config";
pub const SEED_EPOCH: &[u8] = b"epoch";
pub const SEED_CREDIT: &[u8] = b"credit";
pub const SEED_PROP: &[u8] = b"prop";

pub fn config_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SEED_PREFIX, SEED_SEP, SEED_CONFIG], program_id)
}
pub fn epoch_pda(program_id: &Pubkey, epoch: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, SEED_EPOCH, SEED_SEP, &epoch.to_le_bytes()],
        program_id,
    )
}
pub fn credit_pda(program_id: &Pubkey, operator: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, SEED_CREDIT, SEED_SEP, operator.as_ref()],
        program_id,
    )
}
/// recompensa por entrega remota (lamports), por domínio de ENTREGA
pub fn remote_reward_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rrew", SEED_SEP, &domain.to_le_bytes()],
        program_id,
    )
}

/// vínculo de identidade: (domínio de entrega, operador local) → endereço remoto
pub fn remote_binding_pda(program_id: &Pubkey, domain: u32, operator: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rbind", SEED_SEP, &domain.to_le_bytes(), SEED_SEP, operator.as_ref()],
        program_id,
    )
}

pub fn proposal_pda(program_id: &Pubkey, envelope_hash: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, SEED_PROP, SEED_SEP, envelope_hash],
        program_id,
    )
}

// ---------------------------------------------------------------------------
// Estado (espaço fixo com folga — sem realloc)
// ---------------------------------------------------------------------------
pub const MAX_OPERATORS: usize = 16;
pub const CONFIG_SPACE: usize = 1024;
pub const EPOCH_SPACE: usize = 2048;
pub const CREDIT_SPACE: usize = 64;
pub const PROPOSAL_SPACE: usize = 1024;

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct Config {
    pub bump: u8,
    pub quorum: u8,
    pub reward_lamports: u64,
    pub epoch_duration_secs: u64,
    pub paused: bool,
    pub operators: Vec<Pubkey>,
    pub total_credited: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct EpochState {
    pub bump: u8,
    pub epoch: u64,
    /// (slot inicial, slot final) — trava na primeira submissão (spec §09).
    pub window: (u64, u64),
    /// (operador que submeteu, hash do relatório)
    pub submissions: Vec<(Pubkey, [u8; 32])>,
    pub applied: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct OperatorCredit {
    pub bump: u8,
    pub operator: Pubkey,
    pub credited: u64,
    pub withdrawn: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct Proposal {
    pub bump: u8,
    pub envelope_hash: [u8; 32],
    pub approvals: Vec<Pubkey>,
    pub executed: bool,
}

// ---------------------------------------------------------------------------
// Instruções
// ---------------------------------------------------------------------------
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct EpochReport {
    pub epoch: u64,
    pub window_start_slot: u64,
    pub window_end_slot: u64,
    /// (operador, nº de entregas) — ESTRITAMENTE ordenado por operador.
    pub credits: Vec<(Pubkey, u64)>,
    /// v2 ClaimRemote: entregas de msgs ORIGINADAS AQUI feitas pelo operador em
    /// outra chain — (domínio da entrega, operador local, nº de entregas).
    /// Crédito = count × remote_reward(domínio). Coberto pelo MESMO hash/quórum.
    pub remote: Vec<(u32, Pubkey, u64)>,
}

impl EpochReport {
    pub fn hash(&self) -> [u8; 32] {
        let bytes = borsh::to_vec(self).expect("borsh");
        hashv(&[&bytes]).to_bytes()
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub enum AdminAction {
    SetRewardLamports(u64),
    SetQuorum(u8),
    AddOperator(Pubkey),
    RemoveOperator(Pubkey),
    SetPaused(bool),
    /// destino DENTRO do envelope → dentro do hash → dentro da aprovação.
    WithdrawSurplus { to: Pubkey, amount: u64 },
    SetEpochDuration(u64),
    /// v2: recompensa fixa (lamports) por entrega remota no domínio (0 desativa).
    /// Contas extras: [reward PDA w]
    SetRemoteReward { domain: u32, reward: u64 },
    /// v2: vínculo operador local → endereço executor na chain do domínio.
    /// Contas extras: [binding PDA w]
    SetRemoteBinding { domain: u32, operator: Pubkey, remote_address: String },
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct AdminEnvelope {
    /// permite repetir uma ação idêntica no futuro sem colidir com a proposta
    /// anterior já executada (spec §09).
    pub nonce: u64,
    pub action: AdminAction,
}

impl AdminEnvelope {
    pub fn hash(&self) -> [u8; 32] {
        let bytes = borsh::to_vec(self).expect("borsh");
        hashv(&[&bytes]).to_bytes()
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum Instruction {
    /// [payer signer w, config w, system]
    Init {
        operators: Vec<Pubkey>,
        quorum: u8,
        reward_lamports: u64,
        epoch_duration_secs: u64,
    },
    /// [operator signer w (payer), config, epoch w, system, ...credit PDAs w na
    ///  ordem de report.credits]
    SubmitEpochReport { report: EpochReport },
    /// [operator signer w, config w, credit w]
    Withdraw { amount: u64 },
    /// [operator signer w (payer), config w, proposal w, system,
    ///  (WithdrawSurplus: + destino w)]
    SubmitAdminAction { envelope: AdminEnvelope },

    // ---- recibo trustless (registry + destino) ----
    /// [operator s w, config, router PDA w, system]
    SetRemoteRouter { domain: u32, router: [u8; 32] },
    /// [operator s w, config, opsol PDA w, oploc PDA w, system]
    SetOperatorSol { index: u32, operator: Pubkey },
    /// Operador saca o SOL do recibo acumulado na sua PDA operator_sol(index).
    /// [signer(payout) w, opsol PDA(index) w]
    WithdrawOperatorSol { index: u32, amount: u64 },
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------
/// Contas têm espaço fixo com folga (zero-padded); por isso o deserialize é
/// STREAMING (`T::deserialize`), que ignora os bytes finais — `try_from_slice`
/// exigiria consumo exato e falharia.
pub(crate) fn load_streaming<T: BorshDeserialize>(info: &AccountInfo) -> Result<T, ProgramError> {
    let data = info.data.borrow();
    let mut slice: &[u8] = &data;
    T::deserialize(&mut slice).map_err(|_| ProgramError::InvalidAccountData)
}

pub(crate) fn store<T: BorshSerialize>(info: &AccountInfo, value: &T) -> ProgramResult {
    let bytes = borsh::to_vec(value).map_err(|_| ProgramError::InvalidAccountData)?;
    let mut data = info.data.borrow_mut();
    if bytes.len() > data.len() {
        return Err(ProgramError::AccountDataTooSmall);
    }
    data[..bytes.len()].copy_from_slice(&bytes);
    // zera o resto p/ deserialização streaming estável
    for b in data[bytes.len()..].iter_mut() {
        *b = 0;
    }
    Ok(())
}

pub(crate) fn create_pda<'a>(
    payer: &AccountInfo<'a>,
    pda: &AccountInfo<'a>,
    system: &AccountInfo<'a>,
    program_id: &Pubkey,
    space: usize,
    seeds: &[&[u8]],
) -> ProgramResult {
    let rent = Rent::get()?.minimum_balance(space);
    invoke_signed(
        &system_instruction::create_account(payer.key, pda.key, rent, space as u64, program_id),
        &[payer.clone(), pda.clone(), system.clone()],
        &[seeds],
    )
}

pub(crate) fn ensure(cond: bool, err: ProgramError) -> ProgramResult {
    if cond {
        Ok(())
    } else {
        Err(err)
    }
}

const ERR_NOT_OPERATOR: u32 = 100;
const ERR_PAUSED: u32 = 101;
const ERR_WINDOW_MISMATCH: u32 = 102;
const ERR_UNSORTED: u32 = 103;
const ERR_EPOCH_OPEN: u32 = 104;
const ERR_APPLIED: u32 = 105;
const ERR_BAD_QUORUM: u32 = 106;
const ERR_INSUFFICIENT_CREDIT: u32 = 107;
const ERR_POOL_RENT: u32 = 108;
const ERR_EXECUTED: u32 = 109;
const ERR_BAD_DESTINATION: u32 = 110;
const ERR_TOO_MANY: u32 = 111;
const ERR_EPOCH_WRONG_REPORT: u32 = 112;
const ERR_NO_REMOTE_REWARD: u32 = 113;
const ERR_NO_REMOTE_BINDING: u32 = 114;

pub(crate) fn custom(code: u32) -> ProgramError {
    ProgramError::Custom(code)
}

// ---------------------------------------------------------------------------
// Processor
// ---------------------------------------------------------------------------
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let instruction =
        Instruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstructionData)?;
    match instruction {
        Instruction::Init {
            operators,
            quorum,
            reward_lamports,
            epoch_duration_secs,
        } => init(program_id, accounts, operators, quorum, reward_lamports, epoch_duration_secs),
        Instruction::SubmitEpochReport { report } => submit_report(program_id, accounts, report),
        Instruction::Withdraw { amount } => withdraw(program_id, accounts, amount),
        Instruction::SubmitAdminAction { envelope } => {
            submit_admin_action(program_id, accounts, envelope)
        }
        Instruction::SetRemoteRouter { domain, router } => {
            receipt::set_remote_router(program_id, accounts, domain, router)
        }
        Instruction::SetOperatorSol { index, operator } => {
            receipt::set_operator_sol(program_id, accounts, index, operator)
        }
        Instruction::WithdrawOperatorSol { index, amount } => {
            receipt::withdraw_operator_sol(program_id, accounts, index, amount)
        }
    }
}

fn init(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    operators: Vec<Pubkey>,
    quorum: u8,
    reward_lamports: u64,
    epoch_duration_secs: u64,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let payer = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(payer.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(system_program::check_id(system.key), ProgramError::IncorrectProgramId)?;
    ensure(!operators.is_empty() && operators.len() <= MAX_OPERATORS, custom(ERR_TOO_MANY))?;
    ensure(
        quorum >= 1 && (quorum as usize) <= operators.len(),
        custom(ERR_BAD_QUORUM),
    )?;
    ensure(epoch_duration_secs > 0, ProgramError::InvalidInstructionData)?;
    ensure(reward_lamports > 0, ProgramError::InvalidInstructionData)?;

    let (expected, bump) = config_pda(program_id);
    ensure(*config_info.key == expected, ProgramError::InvalidSeeds)?;
    ensure(config_info.data_is_empty(), ProgramError::AccountAlreadyInitialized)?;

    create_pda(
        payer,
        config_info,
        system,
        program_id,
        CONFIG_SPACE,
        &[SEED_PREFIX, SEED_SEP, SEED_CONFIG, &[bump]],
    )?;

    store(
        config_info,
        &Config {
            bump,
            quorum,
            reward_lamports,
            epoch_duration_secs,
            paused: false,
            operators,
            total_credited: 0,
        },
    )
}

fn is_operator(config: &Config, key: &Pubkey) -> bool {
    config.operators.iter().any(|op| op == key)
}

fn submit_report(program_id: &Pubkey, accounts: &[AccountInfo], report: EpochReport) -> ProgramResult {
    let iter = &mut accounts.iter();
    let operator = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let epoch_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(operator.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let mut config: Config = load_streaming(config_info)?;
    ensure(!config.paused, custom(ERR_PAUSED))?;
    ensure(is_operator(&config, operator.key), custom(ERR_NOT_OPERATOR))?;

    // só épocas FECHADAS são reportáveis (a folga de confirmações é operacional)
    let now = Clock::get()?.unix_timestamp as u64;
    let current_epoch = now / config.epoch_duration_secs;
    ensure(report.epoch < current_epoch, custom(ERR_EPOCH_OPEN))?;

    // lista ordenada por chave, sem duplicatas (regra de convergência §09)
    ensure(
        report.credits.windows(2).all(|w| w[0].0 < w[1].0),
        custom(ERR_UNSORTED),
    )?;
    ensure(
        (!report.credits.is_empty() || !report.remote.is_empty())
            && report.credits.len() <= MAX_OPERATORS
            && report.remote.len() <= MAX_OPERATORS,
        custom(ERR_TOO_MANY),
    )?;
    ensure(
        report.window_start_slot <= report.window_end_slot,
        custom(ERR_WINDOW_MISMATCH),
    )?;

    let (expected_epoch, epoch_bump) = epoch_pda(program_id, report.epoch);
    ensure(*epoch_info.key == expected_epoch, ProgramError::InvalidSeeds)?;

    let mut state: EpochState = if epoch_info.data_is_empty() {
        create_pda(
            operator,
            epoch_info,
            system,
            program_id,
            EPOCH_SPACE,
            &[
                SEED_PREFIX,
                SEED_SEP,
                SEED_EPOCH,
                SEED_SEP,
                &report.epoch.to_le_bytes(),
                &[epoch_bump],
            ],
        )?;
        EpochState {
            bump: epoch_bump,
            epoch: report.epoch,
            // primeira submissão TRAVA a janela
            window: (report.window_start_slot, report.window_end_slot),
            submissions: vec![],
            applied: false,
        }
    } else {
        ensure(epoch_info.owner == program_id, ProgramError::IncorrectProgramId)?;
        load_streaming(epoch_info)?
    };

    ensure(!state.applied, custom(ERR_APPLIED))?;
    ensure(
        state.window == (report.window_start_slot, report.window_end_slot),
        custom(ERR_WINDOW_MISMATCH),
    )?;

    let hash = report.hash();
    // sobrescreve a própria submissão, se houver
    state.submissions.retain(|(op, _)| op != operator.key);
    ensure(state.submissions.len() < MAX_OPERATORS, custom(ERR_TOO_MANY))?;
    state.submissions.push((*operator.key, hash));

    let identical = state
        .submissions
        .iter()
        .filter(|(_, h)| *h == hash)
        .count();

    if identical < config.quorum as usize {
        return store(epoch_info, &state);
    }

    // ---- quórum: aplica os créditos DESTE relatório (que gerou o hash) ----
    state.applied = true;
    store(epoch_info, &state)?;

    for (credited_op, delivered) in report.credits.iter() {
        let credit_info = next_account_info(iter).map_err(|_| custom(ERR_EPOCH_WRONG_REPORT))?;
        let (expected_credit, credit_bump) = credit_pda(program_id, credited_op);
        ensure(*credit_info.key == expected_credit, ProgramError::InvalidSeeds)?;

        let mut credit: OperatorCredit = if credit_info.data_is_empty() {
            create_pda(
                operator,
                credit_info,
                system,
                program_id,
                CREDIT_SPACE,
                &[
                    SEED_PREFIX,
                    SEED_SEP,
                    SEED_CREDIT,
                    SEED_SEP,
                    credited_op.as_ref(),
                    &[credit_bump],
                ],
            )?;
            OperatorCredit {
                bump: credit_bump,
                operator: *credited_op,
                credited: 0,
                withdrawn: 0,
            }
        } else {
            ensure(credit_info.owner == program_id, ProgramError::IncorrectProgramId)?;
            load_streaming(credit_info)?
        };

        let amount = delivered
            .checked_mul(config.reward_lamports)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        credit.credited = credit
            .credited
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        config.total_credited = config
            .total_credited
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        store(credit_info, &credit)?;
    }

    // ---- v2 ClaimRemote: créditos por entregas REMOTAS de msgs originadas aqui.
    //      Contas por entrada de report.remote: [reward PDA ro, binding PDA ro,
    //      credit PDA w]. Crédito = count × reward(domínio); saque via Withdraw.
    for (domain, remote_op, count) in report.remote.iter() {
        ensure(*count > 0, ProgramError::InvalidInstructionData)?;

        let reward_info = next_account_info(iter).map_err(|_| custom(ERR_NO_REMOTE_REWARD))?;
        let (expected_rw, _) = remote_reward_pda(program_id, *domain);
        ensure(
            *reward_info.key == expected_rw && reward_info.owner == program_id,
            custom(ERR_NO_REMOTE_REWARD),
        )?;
        let reward: u64 = load_streaming(reward_info)?;
        ensure(reward > 0, custom(ERR_NO_REMOTE_REWARD))?;

        let binding_info = next_account_info(iter).map_err(|_| custom(ERR_NO_REMOTE_BINDING))?;
        let (expected_bind, _) = remote_binding_pda(program_id, *domain, remote_op);
        ensure(
            *binding_info.key == expected_bind && binding_info.owner == program_id,
            custom(ERR_NO_REMOTE_BINDING),
        )?;

        let credit_info = next_account_info(iter).map_err(|_| custom(ERR_EPOCH_WRONG_REPORT))?;
        let (expected_credit, credit_bump) = credit_pda(program_id, remote_op);
        ensure(*credit_info.key == expected_credit, ProgramError::InvalidSeeds)?;
        let mut credit: OperatorCredit = if credit_info.data_is_empty() {
            create_pda(
                operator,
                credit_info,
                system,
                program_id,
                CREDIT_SPACE,
                &[
                    SEED_PREFIX,
                    SEED_SEP,
                    SEED_CREDIT,
                    SEED_SEP,
                    remote_op.as_ref(),
                    &[credit_bump],
                ],
            )?;
            OperatorCredit {
                bump: credit_bump,
                operator: *remote_op,
                credited: 0,
                withdrawn: 0,
            }
        } else {
            ensure(credit_info.owner == program_id, ProgramError::IncorrectProgramId)?;
            load_streaming(credit_info)?
        };
        let amount = count
            .checked_mul(reward)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        credit.credited = credit
            .credited
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        config.total_credited = config
            .total_credited
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        store(credit_info, &credit)?;
    }
    store(config_info, &config)
}

fn withdraw(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let iter = &mut accounts.iter();
    let operator = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let credit_info = next_account_info(iter)?;

    ensure(operator.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    ensure(credit_info.owner == program_id, ProgramError::IncorrectProgramId)?;

    let config: Config = load_streaming(config_info)?;
    ensure(!config.paused, custom(ERR_PAUSED))?;

    let (expected_credit, _) = credit_pda(program_id, operator.key);
    ensure(*credit_info.key == expected_credit, ProgramError::InvalidSeeds)?;

    let mut credit: OperatorCredit = load_streaming(credit_info)?;
    let available = credit
        .credited
        .checked_sub(credit.withdrawn)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    ensure(amount > 0 && amount <= available, custom(ERR_INSUFFICIENT_CREDIT))?;

    // débito direto de lamports RESPEITANDO o rent-exempt da PDA de config
    let rent_floor = Rent::get()?.minimum_balance(config_info.data_len());
    let pool_available = config_info
        .lamports()
        .saturating_sub(rent_floor);
    ensure(amount <= pool_available, custom(ERR_POOL_RENT))?;

    credit.withdrawn = credit
        .withdrawn
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    store(credit_info, &credit)?;

    **config_info.try_borrow_mut_lamports()? -= amount;
    **operator.try_borrow_mut_lamports()? += amount;
    Ok(())
}

fn submit_admin_action(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    envelope: AdminEnvelope,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let operator = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let proposal_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(operator.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let mut config: Config = load_streaming(config_info)?;
    ensure(is_operator(&config, operator.key), custom(ERR_NOT_OPERATOR))?;

    let hash = envelope.hash();
    let (expected_prop, prop_bump) = proposal_pda(program_id, &hash);
    ensure(*proposal_info.key == expected_prop, ProgramError::InvalidSeeds)?;

    let mut proposal: Proposal = if proposal_info.data_is_empty() {
        create_pda(
            operator,
            proposal_info,
            system,
            program_id,
            PROPOSAL_SPACE,
            &[SEED_PREFIX, SEED_SEP, SEED_PROP, SEED_SEP, &hash, &[prop_bump]],
        )?;
        Proposal {
            bump: prop_bump,
            envelope_hash: hash,
            approvals: vec![],
            executed: false,
        }
    } else {
        ensure(proposal_info.owner == program_id, ProgramError::IncorrectProgramId)?;
        load_streaming(proposal_info)?
    };

    ensure(!proposal.executed, custom(ERR_EXECUTED))?;
    if !proposal.approvals.contains(operator.key) {
        ensure(proposal.approvals.len() < MAX_OPERATORS, custom(ERR_TOO_MANY))?;
        proposal.approvals.push(*operator.key);
    }

    if proposal.approvals.len() < config.quorum as usize {
        return store(proposal_info, &proposal);
    }

    // ---- quórum: executa a ação ----
    proposal.executed = true;
    store(proposal_info, &proposal)?;

    match envelope.action {
        AdminAction::SetRewardLamports(v) => {
            ensure(v > 0, ProgramError::InvalidInstructionData)?;
            config.reward_lamports = v;
        }
        AdminAction::SetQuorum(q) => {
            ensure(
                q >= 1 && (q as usize) <= config.operators.len(),
                custom(ERR_BAD_QUORUM),
            )?;
            config.quorum = q;
        }
        AdminAction::AddOperator(op) => {
            if !config.operators.contains(&op) {
                ensure(config.operators.len() < MAX_OPERATORS, custom(ERR_TOO_MANY))?;
                config.operators.push(op);
            }
        }
        AdminAction::RemoveOperator(op) => {
            config.operators.retain(|o| *o != op);
            ensure(!config.operators.is_empty(), custom(ERR_BAD_QUORUM))?;
            ensure(
                (config.quorum as usize) <= config.operators.len(),
                custom(ERR_BAD_QUORUM),
            )?;
        }
        AdminAction::SetPaused(p) => config.paused = p,
        AdminAction::SetEpochDuration(secs) => {
            ensure(secs > 0, ProgramError::InvalidInstructionData)?;
            config.epoch_duration_secs = secs;
        }
        AdminAction::SetRemoteReward { domain, reward } => {
            let reward_info = next_account_info(iter).map_err(|_| custom(ERR_NO_REMOTE_REWARD))?;
            let (expected, bump) = remote_reward_pda(program_id, domain);
            ensure(*reward_info.key == expected, ProgramError::InvalidSeeds)?;
            if reward_info.data_is_empty() {
                create_pda(
                    operator,
                    reward_info,
                    system,
                    program_id,
                    16,
                    &[SEED_PREFIX, SEED_SEP, b"rrew", SEED_SEP, &domain.to_le_bytes(), &[bump]],
                )?;
            }
            store(reward_info, &reward)?;
        }
        AdminAction::SetRemoteBinding { domain, operator: remote_op, remote_address } => {
            ensure(
                !remote_address.is_empty() && remote_address.len() <= 100,
                ProgramError::InvalidInstructionData,
            )?;
            let binding_info = next_account_info(iter).map_err(|_| custom(ERR_NO_REMOTE_BINDING))?;
            let (expected, bump) = remote_binding_pda(program_id, domain, &remote_op);
            ensure(*binding_info.key == expected, ProgramError::InvalidSeeds)?;
            if binding_info.data_is_empty() {
                create_pda(
                    operator,
                    binding_info,
                    system,
                    program_id,
                    128,
                    &[SEED_PREFIX, SEED_SEP, b"rbind", SEED_SEP, &domain.to_le_bytes(), SEED_SEP, remote_op.as_ref(), &[bump]],
                )?;
            }
            store(binding_info, &remote_address)?;
        }
        AdminAction::WithdrawSurplus { to, amount } => {
            // o destino aprovado está DENTRO do hash — a conta passada tem de bater
            let destination = next_account_info(iter).map_err(|_| custom(ERR_BAD_DESTINATION))?;
            ensure(*destination.key == to, custom(ERR_BAD_DESTINATION))?;
            let rent_floor = Rent::get()?.minimum_balance(config_info.data_len());
            let pool_available = config_info.lamports().saturating_sub(rent_floor);
            ensure(amount > 0 && amount <= pool_available, custom(ERR_POOL_RENT))?;
            **config_info.try_borrow_mut_lamports()? -= amount;
            **destination.try_borrow_mut_lamports()? += amount;
        }
    }
    store(config_info, &config)
}
