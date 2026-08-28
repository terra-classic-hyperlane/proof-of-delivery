//! Delivery proof: RAW read of the hpl-mailbox storage.
//!
//! The Mailbox writes `DELIVERIES: Map<Vec<u8>, Delivery>` (cw-storage-plus). The raw
//! key of a `Map` with namespace `"deliveries"` is:
//!
//!   `u16_be(len("deliveries")) + b"deliveries" + message_id`
//!    = `[0x00, 0x0A] + b"deliveries" + 32 bytes`
//!
//! and the value is the JSON of `Delivery { sender: Addr, block_number: u64 }`.
//! Format CONFIRMED against the contract in production (code_id 11371) — see README.

use cosmwasm_std::{Addr, QuerierWrapper};
use serde::Deserialize;

use crate::error::ContractError;

pub const DELIVERIES_NAMESPACE: &[u8] = b"deliveries";

/// STRICT mirror of hpl-mailbox's `Delivery`. `deny_unknown_fields` + required
/// fields: if a migrate adds, removes or renames fields, the parse
/// fails and the vault errors with `MailboxLayoutMismatch` — it never pays based on data
/// it does not understand (spec §06).
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub sender: Addr,
    pub block_number: u64,
}

/// Raw key of the `message_id` entry in the Mailbox's DELIVERIES.
pub fn deliveries_key(message_id: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(2 + DELIVERIES_NAMESPACE.len() + message_id.len());
    key.extend_from_slice(&(DELIVERIES_NAMESPACE.len() as u16).to_be_bytes());
    key.extend_from_slice(DELIVERIES_NAMESPACE);
    key.extend_from_slice(message_id);
    key
}

/// Reads the delivery of `message_id` directly from the Mailbox storage.
/// - `Ok(None)`               → message not delivered (key absent)
/// - `Ok(Some(delivery))`     → delivered; `sender` is the executor
/// - `Err(MailboxLayoutMismatch)` → key exists but the value does not parse (migrate?)
pub fn load_delivery(
    querier: &QuerierWrapper,
    mailbox: &Addr,
    message_id: &[u8],
) -> Result<Option<Delivery>, ContractError> {
    let raw = querier.query_wasm_raw(mailbox, deliveries_key(message_id))?;
    match raw {
        None => Ok(None),
        Some(bytes) => match cosmwasm_std::from_json::<Delivery>(&bytes) {
            Ok(delivery) => Ok(Some(delivery)),
            Err(err) => Err(ContractError::MailboxLayoutMismatch {
                id: hex(message_id),
                reason: err.to_string(),
            }),
        },
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deliveries_key_matches_mainnet_layout() {
        // [0x00, 0x0A] + b"deliveries" + message_id — same as the mainnet dump.
        let id = [0xAB_u8; 32];
        let key = deliveries_key(&id);
        assert_eq!(&key[..2], &[0x00, 0x0A]);
        assert_eq!(&key[2..12], b"deliveries");
        assert_eq!(&key[12..], &id);
        assert_eq!(key.len(), 44);
    }

    #[test]
    fn strict_parse_accepts_production_shape() {
        // Real value observed on mainnet (message d039daa1…4f04).
        let raw = br#"{"sender":"terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp","block_number":29422362}"#;
        let d: Delivery = cosmwasm_std::from_json(raw.as_slice()).unwrap();
        assert_eq!(d.sender.as_str(), "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp");
        assert_eq!(d.block_number, 29422362);
    }

    #[test]
    fn strict_parse_rejects_extra_field() {
        let raw = br#"{"sender":"terra1x","block_number":1,"gas_used":10}"#;
        assert!(cosmwasm_std::from_json::<Delivery>(raw.as_slice()).is_err());
    }

    #[test]
    fn strict_parse_rejects_missing_field() {
        let raw = br#"{"sender":"terra1x"}"#;
        assert!(cosmwasm_std::from_json::<Delivery>(raw.as_slice()).is_err());
    }
}
