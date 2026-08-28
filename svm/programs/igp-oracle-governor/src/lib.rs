//! # IgpOracleGovernor (Solana / Sealevel) — spec §08/§10
//!
//! On Solana the oracle is NOT a separate account: `Igp { owner, beneficiary,
//! gas_oracles }`, and `SetGasOracleConfigs` requires the owner as signer. This
//! program makes its config PDA the **owner of the IGP** and reconstructs the
//! separation of powers with two gates:
//!
//! - **GATE 1 — operators** (quorum + median + bounds): `SubmitPrice`; when
//!   the quorum is reached in the epoch, the median (lower of the central ones on an even tie)
//!   validated against bounds + delta becomes a `SetGasOracleConfigs` CPI signed
//!   by the config PDA.
//! - **GATE 2 — multisig** (single signature): bounds/`token_decimals` per
//!   domain, operators, quorum, delta, `ForceSetGasData`,
//!   `SetIgpBeneficiary` and `TransferIgpOwnership` — the EMERGENCY EXIT that
//!   the spec requires testing before deploy.
//!
//! The config PDA must keep lamports: the IGP realloc charges the owner.
//! The upgrade authority of THIS program must be the multisig (spec §08).

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction as SolInstruction},
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::Sysvar,
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------
pub const SEED_PREFIX: &[u8] = b"gov";
pub const SEED_SEP: &[u8] = b"-";
pub const SEED_CONFIG: &[u8] = b"config";
pub const SEED_DOMAIN: &[u8] = b"domain";
pub const SEED_PRICE: &[u8] = b"price";

pub fn config_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SEED_PREFIX, SEED_SEP, SEED_CONFIG], program_id)
}
pub fn domain_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_SEP, SEED_DOMAIN, SEED_SEP, &domain.to_le_bytes()],
        program_id,
    )
}
// ONE round account per DOMAIN (reused every epoch). The rent is paid only on the 1st creation and
// reused forever — no new PDA per epoch (eliminates rent accumulation across rounds).
pub fn price_round_pda(program_id: &Pubkey, domain: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            SEED_SEP,
            SEED_PRICE,
            SEED_SEP,
            &domain.to_le_bytes(),
        ],
        program_id,
    )
}

pub const MAX_OPERATORS: usize = 16;
pub const CONFIG_SPACE: usize = 1024;
pub const DOMAIN_SPACE: usize = 256;
pub const ROUND_SPACE: usize = 2048;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------
#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct Config {
    pub bump: u8,
    pub multisig: Pubkey,
    pub operators: Vec<Pubkey>,
    pub quorum: u8,
    pub epoch_duration_secs: u64,
    pub max_delta_bps: u64,
    pub igp_program: Pubkey,
    pub igp: Pubkey,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct Bounds {
    pub min_exchange_rate: u128,
    pub max_exchange_rate: u128,
    pub min_gas_price: u128,
    pub max_gas_price: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct DomainState {
    pub bump: u8,
    pub domain: u32,
    pub bounds: Bounds,
    /// network constant — stays with the multisig, OUTSIDE the quorum (spec §08)
    pub token_decimals: u8,
    pub last_rate: u128,
    pub last_gas: u128,
    pub last_set: bool,
    pub last_forced: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct PriceRound {
    pub bump: u8,
    pub domain: u32,
    pub epoch: u64,
    pub submissions: Vec<(Pubkey, u128, u128)>,
    pub applied: bool,
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum Instruction {
    /// [payer s w, config w, system]
    Init {
        multisig: Pubkey,
        operators: Vec<Pubkey>,
        quorum: u8,
        epoch_duration_secs: u64,
        max_delta_bps: u64,
        igp_program: Pubkey,
        igp: Pubkey,
    },
    /// GATE 1 · [operator s w (payer), config w, domain w, round w, system,
    ///            igp_program, igp w]
    SubmitPrice {
        domain: u32,
        token_exchange_rate: u128,
        gas_price: u128,
    },
    /// GATE 2 · [multisig s w (payer), config, domain w, system]
    SetDomainConfig {
        domain: u32,
        bounds: Bounds,
        token_decimals: u8,
    },
    /// GATE 2 · [multisig s, config w]
    SetOperators { add: Vec<Pubkey>, remove: Vec<Pubkey> },
    /// GATE 2 · [multisig s, config w]
    SetQuorum(u8),
    /// GATE 2 · [multisig s, config w]
    SetEpochDuration(u64),
    /// GATE 2 · [multisig s, config w]
    SetMaxDeltaBps(u64),
    /// GATE 2 · [multisig s, config w]
    SetMultisig(Pubkey),
    /// GATE 2 (emergency) · [multisig s, config w, domain w, igp_program, igp w, system]
    ForceSetGasData {
        domain: u32,
        token_exchange_rate: u128,
        gas_price: u128,
    },
    /// GATE 2 · [multisig s, config, igp_program, igp w]
    SetIgpBeneficiary(Pubkey),
    /// GATE 2 (EMERGENCY EXIT) · [multisig s, config, igp_program, igp w]
    TransferIgpOwnership(Option<Pubkey>),
    /// GATE 1 (CLEANUP) · [operator s w (receives the rent), config, round w]
    /// Closes an ORPHAN `round` account (from the old per-epoch ones) and returns 100% of the rent to the operator.
    /// NEVER closes the live account of the domain (the only per-domain one) — guard by address.
    CloseRound,
}

// ---- mirror of the real IGP wire-format (indices 5/7/9) ----
#[derive(BorshSerialize, Debug)]
struct RemoteGasData {
    token_exchange_rate: u128,
    gas_price: u128,
    token_decimals: u8,
}

#[derive(BorshSerialize, Debug)]
enum GasOracle {
    RemoteGasData(RemoteGasData),
}

#[derive(BorshSerialize, Debug)]
struct GasOracleConfig {
    domain: u32,
    gas_oracle: Option<GasOracle>,
}

/// Serializes the IGP instruction with the correct variant index.
fn igp_instruction_data(variant: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + payload.len());
    out.push(variant);
    out.extend_from_slice(payload);
    out
}

const IGP_TRANSFER_OWNERSHIP: u8 = 5;
const IGP_SET_BENEFICIARY: u8 = 7;
const IGP_SET_GAS_ORACLE_CONFIGS: u8 = 9;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------
const ERR_NOT_OPERATOR: u32 = 200;
const ERR_NOT_MULTISIG: u32 = 201;
const ERR_NO_BOUNDS: u32 = 202;
const ERR_OUT_OF_BOUNDS: u32 = 203;
const ERR_APPLIED: u32 = 204;
const ERR_DELTA: u32 = 205;
const ERR_BAD_QUORUM: u32 = 206;
const ERR_TOO_MANY: u32 = 207;
const ERR_BAD_IGP: u32 = 208;
const ERR_ROUND_LIVE: u32 = 209;

fn custom(code: u32) -> ProgramError {
    ProgramError::Custom(code)
}

fn ensure(cond: bool, err: ProgramError) -> ProgramResult {
    if cond {
        Ok(())
    } else {
        Err(err)
    }
}

// ---------------------------------------------------------------------------
// Account helpers
// ---------------------------------------------------------------------------
fn load_streaming<T: BorshDeserialize>(info: &AccountInfo) -> Result<T, ProgramError> {
    let data = info.data.borrow();
    let mut slice: &[u8] = &data;
    T::deserialize(&mut slice).map_err(|_| ProgramError::InvalidAccountData)
}

fn store<T: BorshSerialize>(info: &AccountInfo, value: &T) -> ProgramResult {
    let bytes = borsh::to_vec(value).map_err(|_| ProgramError::InvalidAccountData)?;
    let mut data = info.data.borrow_mut();
    if bytes.len() > data.len() {
        return Err(ProgramError::AccountDataTooSmall);
    }
    data[..bytes.len()].copy_from_slice(&bytes);
    for b in data[bytes.len()..].iter_mut() {
        *b = 0;
    }
    Ok(())
}

fn create_pda<'a>(
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

/// median with "lower of the central ones" tie-break: sorts and takes (n-1)/2
fn lower_median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) / 2]
}

/// |new − last| * 10_000 <= last * max_delta_bps (in improvised u256 via
/// checked u128: the real values fit with room to spare, but we protect with checked)
fn delta_ok(last: u128, new: u128, max_delta_bps: u64) -> bool {
    let diff = last.abs_diff(new);
    match (diff.checked_mul(10_000), last.checked_mul(max_delta_bps as u128)) {
        (Some(lhs), Some(rhs)) => lhs <= rhs,
        _ => false, // overflow on either side → reject (conservative)
    }
}

fn in_bounds(v: u128, min: u128, max: u128) -> bool {
    v >= min && v <= max
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
            multisig,
            operators,
            quorum,
            epoch_duration_secs,
            max_delta_bps,
            igp_program,
            igp,
        } => init(
            program_id, accounts, multisig, operators, quorum, epoch_duration_secs, max_delta_bps,
            igp_program, igp,
        ),
        Instruction::SubmitPrice {
            domain,
            token_exchange_rate,
            gas_price,
        } => submit_price(program_id, accounts, domain, token_exchange_rate, gas_price),
        Instruction::SetDomainConfig {
            domain,
            bounds,
            token_decimals,
        } => set_domain_config(program_id, accounts, domain, bounds, token_decimals),
        Instruction::SetOperators { add, remove } => set_operators(program_id, accounts, add, remove),
        Instruction::SetQuorum(q) => admin_config(program_id, accounts, |c| {
            if q == 0 || (q as usize) > c.operators.len() {
                return Err(custom(ERR_BAD_QUORUM));
            }
            c.quorum = q;
            Ok(())
        }),
        Instruction::SetEpochDuration(secs) => admin_config(program_id, accounts, |c| {
            if secs == 0 {
                return Err(ProgramError::InvalidInstructionData);
            }
            c.epoch_duration_secs = secs;
            Ok(())
        }),
        Instruction::SetMaxDeltaBps(bps) => admin_config(program_id, accounts, |c| {
            c.max_delta_bps = bps;
            Ok(())
        }),
        Instruction::SetMultisig(new_multisig) => admin_config(program_id, accounts, |c| {
            c.multisig = new_multisig;
            Ok(())
        }),
        Instruction::ForceSetGasData {
            domain,
            token_exchange_rate,
            gas_price,
        } => force_set(program_id, accounts, domain, token_exchange_rate, gas_price),
        Instruction::SetIgpBeneficiary(beneficiary) => {
            igp_admin_cpi(program_id, accounts, IGP_SET_BENEFICIARY, &borsh::to_vec(&beneficiary).unwrap())
        }
        Instruction::TransferIgpOwnership(new_owner) => {
            igp_admin_cpi(program_id, accounts, IGP_TRANSFER_OWNERSHIP, &borsh::to_vec(&new_owner).unwrap())
        }
        Instruction::CloseRound => close_round(program_id, accounts),
    }
}

/// Closes an ORPHAN `round` account (from the old per-epoch ones) and returns the rent to the signing operator.
/// Guard: never closes the LIVE account of the domain (the only per-domain one, in the epoch-less PDA).
fn close_round(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let iter = &mut accounts.iter();
    let operator = next_account_info(iter)?; // signer, receives the rent
    let config_info = next_account_info(iter)?;
    let round_info = next_account_info(iter)?;

    ensure(operator.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(
        config.operators.iter().any(|op| op == operator.key),
        custom(ERR_NOT_OPERATOR),
    )?;

    ensure(round_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let round: PriceRound = load_streaming(round_info)?;
    // the live account of the domain stays in the epoch-LESS PDA — protected; we only close the orphans.
    let (live, _) = price_round_pda(program_id, round.domain);
    ensure(*round_info.key != live, custom(ERR_ROUND_LIVE))?;

    close_round_account(round_info, operator)
}

/// Closes `acc` (owned by the program): sends ALL lamports to `dest` and zeroes the data.
/// The runtime collects accounts with 0 lamports at the end of the tx. (Same safe pattern as the vault.)
fn close_round_account(acc: &AccountInfo, dest: &AccountInfo) -> ProgramResult {
    let mut acc_lamports = acc.try_borrow_mut_lamports()?;
    let mut dest_lamports = dest.try_borrow_mut_lamports()?;
    **dest_lamports = dest_lamports
        .checked_add(**acc_lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **acc_lamports = 0;
    for b in acc.try_borrow_mut_data()?.iter_mut() {
        *b = 0;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn init(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    multisig: Pubkey,
    operators: Vec<Pubkey>,
    quorum: u8,
    epoch_duration_secs: u64,
    max_delta_bps: u64,
    igp_program: Pubkey,
    igp: Pubkey,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let payer = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(payer.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(system_program::check_id(system.key), ProgramError::IncorrectProgramId)?;
    ensure(!operators.is_empty() && operators.len() <= MAX_OPERATORS, custom(ERR_TOO_MANY))?;
    ensure(quorum >= 1 && (quorum as usize) <= operators.len(), custom(ERR_BAD_QUORUM))?;
    ensure(epoch_duration_secs > 0, ProgramError::InvalidInstructionData)?;

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
            multisig,
            operators,
            quorum,
            epoch_duration_secs,
            max_delta_bps,
            igp_program,
            igp,
        },
    )
}

fn submit_price(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    domain: u32,
    rate: u128,
    gas: u128,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let operator = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let domain_info = next_account_info(iter)?;
    let round_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;
    let igp_program_info = next_account_info(iter)?;
    let igp_info = next_account_info(iter)?;

    ensure(operator.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(
        config.operators.iter().any(|op| op == operator.key),
        custom(ERR_NOT_OPERATOR),
    )?;
    ensure(*igp_program_info.key == config.igp_program, custom(ERR_BAD_IGP))?;
    ensure(*igp_info.key == config.igp, custom(ERR_BAD_IGP))?;

    // multisig bounds — without them the domain is locked
    ensure(domain_info.owner == program_id, custom(ERR_NO_BOUNDS))?;
    let mut domain_state: DomainState = load_streaming(domain_info)?;
    ensure(domain_state.domain == domain, ProgramError::InvalidSeeds)?;
    let b = domain_state.bounds.clone();
    ensure(
        in_bounds(rate, b.min_exchange_rate, b.max_exchange_rate)
            && in_bounds(gas, b.min_gas_price, b.max_gas_price),
        custom(ERR_OUT_OF_BOUNDS),
    )?;

    let now = Clock::get()?.unix_timestamp as u64;
    let epoch = now / config.epoch_duration_secs;

    let (expected_round, round_bump) = price_round_pda(program_id, domain);
    ensure(*round_info.key == expected_round, ProgramError::InvalidSeeds)?;

    let mut round: PriceRound = if round_info.data_is_empty() {
        // 1st time for the domain: creates the unique account (rent paid only here, reused always).
        create_pda(
            operator,
            round_info,
            system,
            program_id,
            ROUND_SPACE,
            &[
                SEED_PREFIX,
                SEED_SEP,
                SEED_PRICE,
                SEED_SEP,
                &domain.to_le_bytes(),
                &[round_bump],
            ],
        )?;
        PriceRound {
            bump: round_bump,
            domain,
            epoch,
            submissions: vec![],
            applied: false,
        }
    } else {
        ensure(round_info.owner == program_id, ProgramError::IncorrectProgramId)?;
        load_streaming(round_info)?
    };

    // New epoch → resets the window in the SAME account (without creating a new PDA). The on-chain clock only
    // advances, so round.epoch never gets ahead of `epoch` (defensive against that).
    if round.epoch < epoch {
        round.epoch = epoch;
        round.submissions.clear();
        round.applied = false;
    } else {
        ensure(round.epoch == epoch, ProgramError::InvalidInstructionData)?;
    }

    ensure(!round.applied, custom(ERR_APPLIED))?;

    round.submissions.retain(|(op, _, _)| op != operator.key);
    ensure(round.submissions.len() < MAX_OPERATORS, custom(ERR_TOO_MANY))?;
    round.submissions.push((*operator.key, rate, gas));

    if round.submissions.len() < config.quorum as usize {
        return store(round_info, &round);
    }

    // ---- quorum: field-by-field median ----
    let mut rates: Vec<u128> = round.submissions.iter().map(|(_, r, _)| *r).collect();
    let mut gases: Vec<u128> = round.submissions.iter().map(|(_, _, g)| *g).collect();
    let median_rate = lower_median(&mut rates);
    let median_gas = lower_median(&mut gases);

    if domain_state.last_set {
        ensure(
            delta_ok(domain_state.last_rate, median_rate, config.max_delta_bps)
                && delta_ok(domain_state.last_gas, median_gas, config.max_delta_bps),
            custom(ERR_DELTA),
        )?;
    }

    round.applied = true;
    store(round_info, &round)?;

    domain_state.last_rate = median_rate;
    domain_state.last_gas = median_gas;
    domain_state.last_set = true;
    domain_state.last_forced = false;
    store(domain_info, &domain_state)?;

    cpi_set_gas_oracle(
        program_id,
        &config,
        config_info,
        system,
        igp_program_info,
        igp_info,
        domain,
        median_rate,
        median_gas,
        domain_state.token_decimals,
    )
}

#[allow(clippy::too_many_arguments)]
fn cpi_set_gas_oracle<'a>(
    _program_id: &Pubkey,
    config: &Config,
    config_info: &AccountInfo<'a>,
    system: &AccountInfo<'a>,
    igp_program_info: &AccountInfo<'a>,
    igp_info: &AccountInfo<'a>,
    domain: u32,
    rate: u128,
    gas: u128,
    token_decimals: u8,
) -> ProgramResult {
    let configs = vec![GasOracleConfig {
        domain,
        gas_oracle: Some(GasOracle::RemoteGasData(RemoteGasData {
            token_exchange_rate: rate,
            gas_price: gas,
            token_decimals,
        })),
    }];
    let payload = borsh::to_vec(&configs).map_err(|_| ProgramError::InvalidInstructionData)?;
    let instruction = SolInstruction {
        program_id: config.igp_program,
        // accounts of the real IGP: [0 system, 1 igp w, 2 owner signer]
        accounts: vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(*igp_info.key, false),
            AccountMeta::new_readonly(*config_info.key, true),
        ],
        data: igp_instruction_data(IGP_SET_GAS_ORACLE_CONFIGS, &payload),
    };
    invoke_signed(
        &instruction,
        &[system.clone(), igp_info.clone(), config_info.clone(), igp_program_info.clone()],
        &[&[SEED_PREFIX, SEED_SEP, SEED_CONFIG, &[config.bump]]],
    )
}

fn set_domain_config(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    domain: u32,
    bounds: Bounds,
    token_decimals: u8,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let multisig = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let domain_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(multisig.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(*multisig.key == config.multisig, custom(ERR_NOT_MULTISIG))?;
    ensure(
        bounds.min_exchange_rate <= bounds.max_exchange_rate
            && bounds.min_gas_price <= bounds.max_gas_price
            && bounds.max_exchange_rate > 0
            && bounds.max_gas_price > 0,
        ProgramError::InvalidInstructionData,
    )?;

    let (expected, bump) = domain_pda(program_id, domain);
    ensure(*domain_info.key == expected, ProgramError::InvalidSeeds)?;

    let mut state: DomainState = if domain_info.data_is_empty() {
        create_pda(
            multisig,
            domain_info,
            system,
            program_id,
            DOMAIN_SPACE,
            &[
                SEED_PREFIX,
                SEED_SEP,
                SEED_DOMAIN,
                SEED_SEP,
                &domain.to_le_bytes(),
                &[bump],
            ],
        )?;
        DomainState {
            bump,
            domain,
            ..Default::default()
        }
    } else {
        ensure(domain_info.owner == program_id, ProgramError::IncorrectProgramId)?;
        load_streaming(domain_info)?
    };
    state.bounds = bounds;
    state.token_decimals = token_decimals;
    store(domain_info, &state)
}

fn set_operators(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    add: Vec<Pubkey>,
    remove: Vec<Pubkey>,
) -> ProgramResult {
    admin_config(program_id, accounts, |c| {
        for op in add {
            if !c.operators.contains(&op) {
                if c.operators.len() >= MAX_OPERATORS {
                    return Err(custom(ERR_TOO_MANY));
                }
                c.operators.push(op);
            }
        }
        for op in remove {
            c.operators.retain(|o| *o != op);
        }
        if c.operators.is_empty() || (c.quorum as usize) > c.operators.len() {
            return Err(custom(ERR_BAD_QUORUM));
        }
        Ok(())
    })
}

/// [multisig s, config w] + validated mutation
fn admin_config<F>(program_id: &Pubkey, accounts: &[AccountInfo], mutate: F) -> ProgramResult
where
    F: FnOnce(&mut Config) -> ProgramResult,
{
    let iter = &mut accounts.iter();
    let multisig = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;

    ensure(multisig.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let mut config: Config = load_streaming(config_info)?;
    ensure(*multisig.key == config.multisig, custom(ERR_NOT_MULTISIG))?;

    mutate(&mut config)?;
    store(config_info, &config)
}

fn force_set(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    domain: u32,
    rate: u128,
    gas: u128,
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let multisig = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let domain_info = next_account_info(iter)?;
    let igp_program_info = next_account_info(iter)?;
    let igp_info = next_account_info(iter)?;
    let system = next_account_info(iter)?;

    ensure(multisig.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(*multisig.key == config.multisig, custom(ERR_NOT_MULTISIG))?;
    ensure(*igp_program_info.key == config.igp_program, custom(ERR_BAD_IGP))?;
    ensure(*igp_info.key == config.igp, custom(ERR_BAD_IGP))?;
    ensure(domain_info.owner == program_id, custom(ERR_NO_BOUNDS))?;

    let mut domain_state: DomainState = load_streaming(domain_info)?;
    ensure(domain_state.domain == domain, ProgramError::InvalidSeeds)?;

    domain_state.last_rate = rate;
    domain_state.last_gas = gas;
    domain_state.last_set = true;
    domain_state.last_forced = true;
    store(domain_info, &domain_state)?;

    cpi_set_gas_oracle(
        program_id,
        &config,
        config_info,
        system,
        igp_program_info,
        igp_info,
        domain,
        rate,
        gas,
        domain_state.token_decimals,
    )
}

/// administrative CPI (variants 5 and 7): [0 igp w, 1 owner=config PDA signer]
fn igp_admin_cpi(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    variant: u8,
    payload: &[u8],
) -> ProgramResult {
    let iter = &mut accounts.iter();
    let multisig = next_account_info(iter)?;
    let config_info = next_account_info(iter)?;
    let igp_program_info = next_account_info(iter)?;
    let igp_info = next_account_info(iter)?;

    ensure(multisig.is_signer, ProgramError::MissingRequiredSignature)?;
    ensure(config_info.owner == program_id, ProgramError::IncorrectProgramId)?;
    let config: Config = load_streaming(config_info)?;
    ensure(*multisig.key == config.multisig, custom(ERR_NOT_MULTISIG))?;
    ensure(*igp_program_info.key == config.igp_program, custom(ERR_BAD_IGP))?;
    ensure(*igp_info.key == config.igp, custom(ERR_BAD_IGP))?;

    let instruction = SolInstruction {
        program_id: config.igp_program,
        accounts: vec![
            AccountMeta::new(*igp_info.key, false),
            AccountMeta::new_readonly(*config_info.key, true),
        ],
        data: igp_instruction_data(variant, payload),
    };
    invoke_signed(
        &instruction,
        &[igp_info.clone(), config_info.clone(), igp_program_info.clone()],
        &[&[SEED_PREFIX, SEED_SEP, SEED_CONFIG, &[config.bump]]],
    )
}
