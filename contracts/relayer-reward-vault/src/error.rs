use cosmwasm_std::{Addr, StdError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("unauthorized: sender is not the owner")]
    Unauthorized {},

    #[error("vault is paused")]
    Paused {},

    #[error("empty message id batch")]
    EmptyBatch {},

    #[error("duplicated message id in batch: {id}")]
    DuplicatedId { id: String },

    #[error("invalid message id length: expected 32 bytes, got {len}")]
    InvalidMessageId { len: usize },

    #[error("message not delivered: {id}")]
    NotDelivered { id: String },

    // O storage do Mailbox retornou algo que NÃO parseia como `Delivery { sender,
    // block_number }` — provável migrate mudando o layout. Falha explícita em vez
    // de pagar errado (spec §06).
    #[error("mailbox storage layout mismatch for message {id}: {reason}")]
    MailboxLayoutMismatch { id: String, reason: String },

    #[error("message {id} was processed by {processor}, not by the claimer")]
    NotProcessor { id: String, processor: Addr },

    #[error("claim window expired for message {id}: delivered at block {delivered_at}, window ends at {deadline}, current {current}")]
    ClaimWindowExpired {
        id: String,
        delivered_at: u64,
        deadline: u64,
        current: u64,
    },

    #[error("message already claimed: {id} (by {claimant})")]
    AlreadyClaimed { id: String, claimant: Addr },

    #[error("insufficient pool: need {needed}{denom}, pool has {available}{denom} — run Sweep or wait for funding")]
    InsufficientPool {
        needed: String,
        available: String,
        denom: String,
    },

    #[error("reward_per_delivery must be greater than zero")]
    ZeroReward {},

    #[error("claim_window_blocks must be greater than zero")]
    ZeroWindow {},

    #[error("withdraw amount must be greater than zero")]
    ZeroWithdraw {},

    // ---- v2 ClaimRemote ----
    #[error("sender is not a registered remote attestor")]
    NotAttestor {},

    #[error("no remote binding for operator {operator} on domain {domain}")]
    NoBinding { operator: String, domain: u32 },

    #[error("no remote reward configured for domain {domain}")]
    NoRemoteReward { domain: u32 },

    #[error("remote delivery already paid: {id} (to {executor})")]
    RemoteAlreadyClaimed { id: String, executor: String },

    #[error("attestor already attested message {id}")]
    AlreadyAttested { id: String },

    #[error("remote quorum must be >= 1 and <= number of attestors")]
    BadRemoteQuorum {},

    #[error("nothing new to send: all message ids in the batch already had a receipt dispatched")]
    NothingNewToSend {},
}
