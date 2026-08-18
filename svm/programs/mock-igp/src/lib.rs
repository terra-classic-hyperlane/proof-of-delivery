//! Mock do hyperlane-sealevel-igp para os testes do governor.
//!
//! Espelha o que importa do programa real:
//! - enum de instruções com os MESMOS índices borsh
//!   (TransferIgpOwnership=5 · SetIgpBeneficiary=7 · SetGasOracleConfigs=9);
//! - layout de contas idêntico ao processor.rs real:
//!   5/7: [0 igp w, 1 owner signer] · 9: [0 system, 1 igp w, 2 owner signer];
//! - `ensure_owner_signer`: o signer tem de ser o `owner` gravado no estado.
//!
//! O estado do mock é próprio (o governor nunca lê o estado do IGP, só faz CPI).

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

// ---- tipos com o mesmo wire-format do programa real ----

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteGasData {
    pub token_exchange_rate: u128,
    pub gas_price: u128,
    pub token_decimals: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub enum GasOracle {
    RemoteGasData(RemoteGasData),
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct GasOracleConfig {
    pub domain: u32,
    pub gas_oracle: Option<GasOracle>,
}

// placeholders só para manter os índices das variantes não usadas
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct Unused;

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum Instruction {
    Init,                                        // 0
    InitIgp(Unused),                             // 1
    InitOverheadIgp(Unused),                     // 2
    PayForGas(Unused),                           // 3
    QuoteGasPayment(Unused),                     // 4
    TransferIgpOwnership(Option<Pubkey>),        // 5
    TransferOverheadIgpOwnership(Option<Pubkey>),// 6
    SetIgpBeneficiary(Pubkey),                   // 7
    SetDestinationGasOverheads(Vec<Unused>),     // 8
    SetGasOracleConfigs(Vec<GasOracleConfig>),   // 9
    Claim,                                       // 10
}

/// Estado do IGP mockado (a conta é pré-populada pelos testes).
#[derive(BorshSerialize, BorshDeserialize, Debug, Default)]
pub struct MockIgpState {
    pub owner: Option<Pubkey>,
    pub beneficiary: Pubkey,
    pub oracles: Vec<(u32, RemoteGasData)>,
}

fn load(info: &AccountInfo) -> Result<MockIgpState, ProgramError> {
    let data = info.data.borrow();
    let mut slice: &[u8] = &data;
    MockIgpState::deserialize(&mut slice).map_err(|_| ProgramError::InvalidAccountData)
}

fn store(info: &AccountInfo, state: &MockIgpState) -> ProgramResult {
    let bytes = borsh::to_vec(state).map_err(|_| ProgramError::InvalidAccountData)?;
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

fn ensure_owner_signer(state: &MockIgpState, owner_info: &AccountInfo) -> ProgramResult {
    if !owner_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    match state.owner {
        Some(owner) if owner == *owner_info.key => Ok(()),
        _ => Err(ProgramError::IllegalOwner),
    }
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let instruction =
        Instruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstructionData)?;
    let iter = &mut accounts.iter();
    match instruction {
        // [0 igp w, 1 owner signer]
        Instruction::TransferIgpOwnership(new_owner) => {
            let igp_info = next_account_info(iter)?;
            let owner_info = next_account_info(iter)?;
            if igp_info.owner != program_id {
                return Err(ProgramError::IncorrectProgramId);
            }
            let mut state = load(igp_info)?;
            ensure_owner_signer(&state, owner_info)?;
            state.owner = new_owner;
            store(igp_info, &state)
        }
        // [0 igp w, 1 owner signer]
        Instruction::SetIgpBeneficiary(beneficiary) => {
            let igp_info = next_account_info(iter)?;
            let owner_info = next_account_info(iter)?;
            if igp_info.owner != program_id {
                return Err(ProgramError::IncorrectProgramId);
            }
            let mut state = load(igp_info)?;
            ensure_owner_signer(&state, owner_info)?;
            state.beneficiary = beneficiary;
            store(igp_info, &state)
        }
        // [0 system, 1 igp w, 2 owner signer]
        Instruction::SetGasOracleConfigs(configs) => {
            let _system = next_account_info(iter)?;
            let igp_info = next_account_info(iter)?;
            let owner_info = next_account_info(iter)?;
            if igp_info.owner != program_id {
                return Err(ProgramError::IncorrectProgramId);
            }
            let mut state = load(igp_info)?;
            ensure_owner_signer(&state, owner_info)?;
            for cfg in configs {
                state.oracles.retain(|(d, _)| *d != cfg.domain);
                if let Some(GasOracle::RemoteGasData(data)) = cfg.gas_oracle {
                    state.oracles.push((cfg.domain, data));
                }
            }
            store(igp_info, &state)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
