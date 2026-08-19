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
    use rrv::receipt;
    // O Mailbox chama o recipient (nós) com um discriminador de 8 bytes da
    // interface MessageRecipient — tratamos ANTES do roteamento por módulo.
    if let Some(disc) = receipt::recipient_discriminator(data) {
        if disc == receipt::HANDLE_DISC {
            // borsh HandleInstruction { origin u32, sender H256(32), message Vec }
            let rest = &data[8..];
            let origin = u32::from_le_bytes(
                rest.get(0..4).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap(),
            );
            let sender: [u8; 32] = rest
                .get(4..36)
                .ok_or(ProgramError::InvalidInstructionData)?
                .try_into()
                .unwrap();
            let mlen = u32::from_le_bytes(
                rest.get(36..40).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap(),
            ) as usize;
            let message = rest
                .get(40..40 + mlen)
                .ok_or(ProgramError::InvalidInstructionData)?;
            return receipt::handle(program_id, accounts, origin, sender, message);
        }
        if disc == receipt::ISM_DISC {
            return receipt::ism_response();
        }
        // HandleAccountMetas / IsmAccountMetas → metas vazias (o keeper monta)
        return receipt::empty_metas();
    }
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
