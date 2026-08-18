//! pod — vault (rrv) + igp-oracle-governor num ÚNICO programa Solana.
//!
//! Motivo: rent é cobrado por byte e ~90% de cada .so é a runtime
//! solana-program+borsh, idêntica nos dois. Fundidos, ela é paga UMA vez
//! (~150 KB no total em vez de ~260 KB → ~0,8 SOL a menos de caução).
//!
//! Roteamento: o PRIMEIRO byte do instruction data escolhe o módulo e o
//! restante é o instruction data original daquele programa:
//!   0x00 → rrv (vault)   ·   0x01 → igp-oracle-governor
//!
//! As PDAs dos dois módulos convivem sob o MESMO program id sem colisão
//! (seeds já são namespaced: "rrv-config" vs "gov-…").
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

pub const MODULE_RRV: u8 = 0;
pub const MODULE_GOV: u8 = 1;

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let (module, rest) = data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    match *module {
        MODULE_RRV => rrv::process_instruction(program_id, accounts, rest),
        MODULE_GOV => igp_oracle_governor::process_instruction(program_id, accounts, rest),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejeita_modulo_desconhecido_e_data_vazio() {
        let pid = Pubkey::new_unique();
        assert_eq!(
            process_instruction(&pid, &[], &[9]),
            Err(ProgramError::InvalidInstructionData)
        );
        assert_eq!(
            process_instruction(&pid, &[], &[]),
            Err(ProgramError::InvalidInstructionData)
        );
    }
}
