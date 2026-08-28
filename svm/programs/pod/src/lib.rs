//! pod — vault (rrv) + igp-oracle-governor in a SINGLE Solana program.
//!
//! Reason: rent is charged per byte and ~90% of each .so is the
//! solana-program+borsh runtime, identical in both. Merged, it is paid ONCE
//! (~150 KB total instead of ~260 KB → ~0.8 SOL less collateral).
//!
//! Routing: the FIRST byte of the instruction data chooses the module and the
//! rest is the original instruction data of that program:
//!   0x00 → rrv (vault)   ·   0x01 → igp-oracle-governor
//!
//! The PDAs of the two modules coexist under the SAME program id without collision
//! (seeds are already namespaced: "rrv-config" vs "gov-…").
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
    // The Mailbox calls the recipient (us) with an 8-byte discriminator of the
    // MessageRecipient interface — we handle it BEFORE the per-module routing.
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
        if disc == receipt::ISM_METAS_DISC {
            return receipt::ism_account_metas();
        }
        if disc == receipt::HANDLE_METAS_DISC {
            // borsh HandleInstruction { origin u32, sender H256(32), message Vec }
            let rest = &data[8..];
            let origin = u32::from_le_bytes(
                rest.get(0..4).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap(),
            );
            let mlen = u32::from_le_bytes(
                rest.get(36..40).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap(),
            ) as usize;
            let message = rest
                .get(40..40 + mlen)
                .ok_or(ProgramError::InvalidInstructionData)?;
            return receipt::handle_account_metas(program_id, origin, message);
        }
        return Err(ProgramError::InvalidInstructionData);
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
    fn rejects_unknown_module_and_empty_data() {
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
