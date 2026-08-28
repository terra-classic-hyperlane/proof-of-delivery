//! # RelayerRewardVault (Solana / Sealevel) — spec §08/§09
//!
//! The config PDA **is** the pool: register it as the IGP `beneficiary` and the
//! lamports from the IGP `Claim` accumulate in it. Since the Solana Mailbox does NOT
//! record who executed `process()` (the `ProcessedMessage` has no executor
//! field), authorship comes from a **per-epoch operator quorum**:
//!
//! - Each operator submits the SAME epoch report (credit list
//!   sorted by key — convergence rule of spec §09); the borsh hash
//!   is compared; a quorum of identical hashes → credits attributed.
//! - The slot window becomes **immutable** after the first submission.
//! - Divergence → the epoch simply does not close (the alarm is off-chain).
//! - Withdrawal: direct debit of pool lamports, respecting the rent-exempt floor.
//! - Administration WITHOUT a single admin: `AdminEnvelope { nonce, action }`, with the
//!   proposal PDA seeded by the envelope hash (everyone converges on the same
//!   account) and execution at quorum. In `WithdrawSurplus` the destination is part of the
//!   hash — THAT destination is what gets approved.
//!
//! This program does NOT touch the Mailbox nor the IGP.

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
/// reward per remote delivery (lamports), per DELIVERY domain
pub fn remote_reward_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, b"rrew", SEED_SEP, &domain.to_le_bytes()],
        program_id,
    )
}

/// identity binding: (delivery domain, local operator) → remote address
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
// State (fixed space with slack — no realloc)
// ---------------------------------------------------------------------------
pub const MAX_OPERATORS: usize = 16;
pub const CONFIG_SPACE: usize = 1024;
pub const CREDIT_SPACE: usize = 64;
pub const PROPOSAL_SPACE: usize = 1024;

/// Anti-replay guard window in BITS (1 bit per epoch). 512 bits × 6h = 128
/// days of slack — far beyond the reporter's hourly cycle. It NEVER slides
/// on its own (avoids silently losing credit); the base only advances through
/// governance via `SetAppliedBase` (monotonic: forward only).
pub const APPLIED_WINDOW_BITS: usize = 512;
pub const APPLIED_WINDOW_BYTES: usize = APPLIED_WINDOW_BITS / 8; // 64

fn bit_get(bm: &[u8; APPLIED_WINDOW_BYTES], i: usize) -> bool {
    (bm[i / 8] >> (i % 8)) & 1 == 1
}
fn bit_set(bm: &mut [u8; APPLIED_WINDOW_BYTES], i: usize) {
    bm[i / 8] |= 1 << (i % 8);
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct Config {
    pub bump: u8,
    pub quorum: u8,
    pub reward_lamports: u64,
    pub epoch_duration_secs: u64,
    pub paused: bool,
    pub operators: Vec<Pubkey>,
    pub total_credited: u64,
    // ---- epoch anti-replay guard (append-only for migration compat:
    //      an old Config deserializes these fields as zeros) ----
    /// oldest epoch still "rememberable"; epochs < base are REJECTED
    /// (window has passed). Migration: stays 0 until governance calls SetAppliedBase.
    pub applied_base: u64,
    /// 1 bit per epoch starting from `applied_base`. bit set = epoch already paid.
    pub applied_bitmap: [u8; APPLIED_WINDOW_BYTES],
}

/// exact size of the epoch collection account for `n` possible submitters
/// (= number of operators). Replaces the fixed 2048 EPOCH_SPACE — right-sizing.
pub fn epoch_space(num_operators: usize) -> usize {
    // bump(1)+epoch(8)+window(16)+veclen(4)+n*(32+32)+applied(1)
    1 + 8 + 16 + 4 + num_operators * 64 + 1
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct EpochState {
    pub bump: u8,
    pub epoch: u64,
    /// (start slot, end slot) — locks on the first submission (spec §09).
    pub window: (u64, u64),
    /// (operator who submitted, report hash)
    pub submissions: Vec<(Pubkey, [u8; 32])>,
    /// true only between setting the bit and closing the account in the SAME tx (does not persist:
    /// the account is closed right after). The real replay guard is the Config bitmap.
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
// Instructions
// ---------------------------------------------------------------------------
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct EpochReport {
    pub epoch: u64,
    pub window_start_slot: u64,
    pub window_end_slot: u64,
    /// (operator, number of deliveries) — STRICTLY sorted by operator.
    pub credits: Vec<(Pubkey, u64)>,
    /// v2 ClaimRemote: deliveries of msgs ORIGINATED HERE made by the operator on
    /// another chain — (delivery domain, local operator, number of deliveries).
    /// Credit = count × remote_reward(domain). Covered by the SAME hash/quorum.
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
    /// destination INSIDE the envelope → inside the hash → inside the approval.
    WithdrawSurplus { to: Pubkey, amount: u64 },
    SetEpochDuration(u64),
    /// v2: fixed reward (lamports) per remote delivery on the domain (0 disables).
    /// Extra accounts: [reward PDA w]
    SetRemoteReward { domain: u32, reward: u64 },
    /// v2: binding local operator → executor address on the domain's chain.
    /// Extra accounts: [binding PDA w]
    SetRemoteBinding { domain: u32, operator: Pubkey, remote_address: String },
    /// Advances the base of the epoch replay window. MONOTONIC: forward only
    /// (never goes back — going back would reopen already-paid epochs = double-payment).
    /// Use 1 (migration): an old Config has base=0; governance sets the base to the
    /// current epoch before the new submissions. Use 2: free up space in the window
    /// when it nears the end (128 days) — only by discarding old epochs
    /// that governance confirms as already settled.
    SetAppliedBase(u64),
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct AdminEnvelope {
    /// allows repeating an identical action in the future without colliding with the
    /// previous, already-executed proposal (spec §09).
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
    /// [operator signer w (payer), config, epoch w, system, ...credit PDAs w in
    ///  the order of report.credits]
    SubmitEpochReport { report: EpochReport },
    /// [operator signer w, config w, credit w]
    Withdraw { amount: u64 },
    /// [operator signer w (payer), config w, proposal w, system,
    ///  (WithdrawSurplus: + destination w)]
    SubmitAdminAction { envelope: AdminEnvelope },

    // ---- trustless receipt (registry + destination) ----
    /// [operator s w, config, router PDA w, system]
    SetRemoteRouter { domain: u32, router: [u8; 32] },
    /// [operator s w, config, opsol PDA w, oploc PDA w, system]
    SetOperatorSol { index: u32, operator: Pubkey },
    /// Operator withdraws the receipt SOL accumulated in its operator_sol(index) PDA.
    /// [signer(payout) w, opsol PDA(index) w]
    WithdrawOperatorSol { index: u32, amount: u64 },
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------
/// Accounts have fixed space with slack (zero-padded); that is why deserialize is
/// STREAMING (`T::deserialize`), which ignores the trailing bytes — `try_from_slice`
/// would require exact consumption and fail.
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
    // zero the rest for stable streaming deserialization
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
const ERR_EPOCH_TOO_OLD: u32 = 115;    // epoch < applied_base (window has already passed)
const ERR_EPOCH_TOO_FUTURE: u32 = 116; // epoch >= base + window (governance must advance the base)
const ERR_BASE_NOT_MONOTONIC: u32 = 117; // SetAppliedBase only advances, never goes back

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
            // replay guard starts EMPTY. The base TRAILS BEHIND now (half a
            // window) — otherwise the last closed epoch (current−1, the one the
            // reporter reports) would fall below the base. This leaves ~64 days of
            // back-report behind and ~64 days of slack ahead.
            applied_base: {
                let now = Clock::get()?.unix_timestamp as u64;
                (now / epoch_duration_secs).saturating_sub((APPLIED_WINDOW_BITS / 2) as u64)
            },
            applied_bitmap: [0u8; APPLIED_WINDOW_BYTES],
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

    // only CLOSED epochs are reportable (the confirmation slack is operational)
    let now = Clock::get()?.unix_timestamp as u64;
    let current_epoch = now / config.epoch_duration_secs;
    ensure(report.epoch < current_epoch, custom(ERR_EPOCH_OPEN))?;

    // list sorted by key, no duplicates (convergence rule §09)
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

    // ---- ANTI-REPLAY GUARD (bitmap in Config) — checked BEFORE any
    //      creation/write: an already-paid epoch is rejected immediately, even if
    //      its collection account has already been closed. ----
    ensure(report.epoch >= config.applied_base, custom(ERR_EPOCH_TOO_OLD))?;
    let bit_off = (report.epoch - config.applied_base) as usize;
    ensure(bit_off < APPLIED_WINDOW_BITS, custom(ERR_EPOCH_TOO_FUTURE))?;
    ensure(!bit_get(&config.applied_bitmap, bit_off), custom(ERR_APPLIED))?;

    let (expected_epoch, epoch_bump) = epoch_pda(program_id, report.epoch);
    ensure(*epoch_info.key == expected_epoch, ProgramError::InvalidSeeds)?;

    let mut state: EpochState = if epoch_info.data_is_empty() {
        create_pda(
            operator,
            epoch_info,
            system,
            program_id,
            // right-sizing: fits exactly 1 submission per existing operator
            epoch_space(config.operators.len()),
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
            // first submission LOCKS the window
            window: (report.window_start_slot, report.window_end_slot),
            submissions: vec![],
            applied: false,
        }
    } else {
        ensure(epoch_info.owner == program_id, ProgramError::IncorrectProgramId)?;
        load_streaming(epoch_info)?
    };

    // state.applied only exists INSIDE the application tx (the account is closed
    // right after); the lasting guard is the bitmap, already checked above.
    ensure(
        state.window == (report.window_start_slot, report.window_end_slot),
        custom(ERR_WINDOW_MISMATCH),
    )?;

    let hash = report.hash();
    // overwrites its own submission, if any
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

    // ---- quorum: applies the credits of THIS report (the one that generated the hash) ----
    // Sets the replay bit in Config BEFORE distributing. Everything in this instruction
    // is ATOMIC: if any distribution/persistence fails, the bit also
    // reverts — it is never left "half paid". The bit persists with the final store(config).
    bit_set(&mut config.applied_bitmap, bit_off);

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

    // ---- v2 ClaimRemote: credits for REMOTE deliveries of msgs originated here.
    //      Accounts per report.remote entry: [reward PDA ro, binding PDA ro,
    //      credit PDA w]. Credit = count × reward(domain); withdrawal via Withdraw.
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

    // ---- CLOSES the collection account and returns 100% of the rent to the
    //      signing operator. The replay guard now lives in the Config bitmap (set
    //      above), so the account no longer needs to exist — we reclaim the rent.
    //      Safe close pattern: zeroes the data, transfers ALL lamports to the
    //      destination and leaves the account with 0 lamports (the runtime collects it at the end of the
    //      tx). All atomic with setting the bit and the distribution. ----
    close_account(epoch_info, operator)?;

    store(config_info, &config)
}

/// Closes `acc` (owned by the program) by sending all lamports to `dest` and
/// zeroing the data. The runtime removes accounts with 0 lamports at the end of the tx.
fn close_account(acc: &AccountInfo, dest: &AccountInfo) -> ProgramResult {
    let mut acc_lamports = acc.try_borrow_mut_lamports()?;
    let mut dest_lamports = dest.try_borrow_mut_lamports()?;
    **dest_lamports = dest_lamports
        .checked_add(**acc_lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **acc_lamports = 0;
    // zero the data (defense: nothing readable/reusable is left behind)
    for b in acc.try_borrow_mut_data()?.iter_mut() {
        *b = 0;
    }
    Ok(())
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

    // direct lamport debit RESPECTING the rent-exempt floor of the config PDA
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

    // ---- quorum: executes the action ----
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
            // the approved destination is INSIDE the hash — the passed account must match
            let destination = next_account_info(iter).map_err(|_| custom(ERR_BAD_DESTINATION))?;
            ensure(*destination.key == to, custom(ERR_BAD_DESTINATION))?;
            let rent_floor = Rent::get()?.minimum_balance(config_info.data_len());
            let pool_available = config_info.lamports().saturating_sub(rent_floor);
            ensure(amount > 0 && amount <= pool_available, custom(ERR_POOL_RENT))?;
            **config_info.try_borrow_mut_lamports()? -= amount;
            **destination.try_borrow_mut_lamports()? += amount;
        }
        AdminAction::SetAppliedBase(new_base) => {
            // MONOTONIC: only advances. Going back would reopen epochs already marked in the
            // bitmap = double-payment risk — forbidden.
            ensure(new_base >= config.applied_base, custom(ERR_BASE_NOT_MONOTONIC))?;
            // when advancing the base, the bitmap shifts: the bits of epochs that left
            // the window are discarded (but they stay protected by ERR_EPOCH_TOO_OLD,
            // since now epoch < base). Shift by (new_base - base) bits.
            let shift = (new_base - config.applied_base) as usize;
            if shift >= APPLIED_WINDOW_BITS {
                config.applied_bitmap = [0u8; APPLIED_WINDOW_BYTES];
            } else if shift > 0 {
                let mut nb = [0u8; APPLIED_WINDOW_BYTES];
                for i in shift..APPLIED_WINDOW_BITS {
                    if bit_get(&config.applied_bitmap, i) {
                        bit_set(&mut nb, i - shift);
                    }
                }
                config.applied_bitmap = nb;
            }
            config.applied_base = new_base;
        }
    }
    store(config_info, &config)
}
