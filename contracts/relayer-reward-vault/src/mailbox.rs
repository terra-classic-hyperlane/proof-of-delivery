//! Prova de entrega: leitura BRUTA do storage do hpl-mailbox.
//!
//! O Mailbox grava `DELIVERIES: Map<Vec<u8>, Delivery>` (cw-storage-plus). A chave
//! bruta de um `Map` com namespace `"deliveries"` é:
//!
//!   `u16_be(len("deliveries")) + b"deliveries" + message_id`
//!    = `[0x00, 0x0A] + b"deliveries" + 32 bytes`
//!
//! e o valor é o JSON de `Delivery { sender: Addr, block_number: u64 }`.
//! Formato CONFIRMADO contra o contrato em produção (code_id 11371) — ver README.

use cosmwasm_std::{Addr, QuerierWrapper};
use serde::Deserialize;

use crate::error::ContractError;

pub const DELIVERIES_NAMESPACE: &[u8] = b"deliveries";

/// Espelho ESTRITO do `Delivery` do hpl-mailbox. `deny_unknown_fields` + campos
/// obrigatórios: se um migrate acrescentar, remover ou renomear campos, o parse
/// falha e o vault erra com `MailboxLayoutMismatch` — nunca paga com base em dado
/// que não entende (spec §06).
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub sender: Addr,
    pub block_number: u64,
}

/// Chave bruta da entrada de `message_id` no DELIVERIES do Mailbox.
pub fn deliveries_key(message_id: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(2 + DELIVERIES_NAMESPACE.len() + message_id.len());
    key.extend_from_slice(&(DELIVERIES_NAMESPACE.len() as u16).to_be_bytes());
    key.extend_from_slice(DELIVERIES_NAMESPACE);
    key.extend_from_slice(message_id);
    key
}

/// Lê a entrega de `message_id` direto do storage do Mailbox.
/// - `Ok(None)`               → mensagem não entregue (chave ausente)
/// - `Ok(Some(delivery))`     → entregue; `sender` é o executor
/// - `Err(MailboxLayoutMismatch)` → chave existe mas o valor não parseia (migrate?)
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
        // [0x00, 0x0A] + b"deliveries" + message_id — igual ao dump de mainnet.
        let id = [0xAB_u8; 32];
        let key = deliveries_key(&id);
        assert_eq!(&key[..2], &[0x00, 0x0A]);
        assert_eq!(&key[2..12], b"deliveries");
        assert_eq!(&key[12..], &id);
        assert_eq!(key.len(), 44);
    }

    #[test]
    fn strict_parse_accepts_production_shape() {
        // Valor real observado em mainnet (message d039daa1…4f04).
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
