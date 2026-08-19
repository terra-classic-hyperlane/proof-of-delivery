//! # relayer-reward-vault
//!
//! Vault beneficiary do IGP no Terra Classic. Um relayer NÃO recebe por estar numa
//! lista: recebe porque o storage do Mailbox registra que foi ELE quem executou o
//! `process()` da mensagem (`DELIVERIES: Map<Vec<u8>, Delivery { sender, block_number }>`).
//!
//! A prova é uma raw query na chave bruta do cw-storage-plus:
//!   `[len_be_u16] + b"deliveries" + message_id(32)`  →  JSON `{"sender","block_number"}`
//! (verificado contra o wasm EM PRODUÇÃO — code_id 11371, ver README do repositório).
//!
//! Invariantes:
//! - Claim em lote é ATÔMICO: um id inválido reverte tudo.
//! - Parse do valor é ESTRITO (`deny_unknown_fields`): se um migrate do Mailbox mudar
//!   o layout, o contrato falha com `MailboxLayoutMismatch` em vez de pagar errado.
//! - `Sweep` é permissionless: qualquer um pode mandar o vault puxar o saldo do IGP
//!   (o IGP só aceita `claim()` vindo do beneficiary — que é este contrato).
//! - Owner é a governança on-chain do Terra Classic, não multisig.

pub mod error;
pub mod execute;
pub mod mailbox;
pub mod msg;
pub mod query;
pub mod state;

pub use crate::error::ContractError;

#[cfg(not(feature = "library"))]
pub mod entry {
    use cosmwasm_std::{
        entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    };

    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};

    pub const CONTRACT_NAME: &str = "crates.io:relayer-reward-vault";
    pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

    #[entry_point]
    pub fn instantiate(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        crate::execute::instantiate(deps, env, info, msg)
    }

    #[entry_point]
    pub fn execute(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<Response, ContractError> {
        match msg {
            ExecuteMsg::Claim { message_ids } => crate::execute::claim(deps, env, info, message_ids),
            ExecuteMsg::Sweep {} => crate::execute::sweep(deps, env, info),
            ExecuteMsg::UpdateConfig {
                owner,
                mailbox,
                igp,
                reward_per_delivery,
                claim_window_blocks,
            } => crate::execute::update_config(
                deps,
                info,
                owner,
                mailbox,
                igp,
                reward_per_delivery,
                claim_window_blocks,
            ),
            ExecuteMsg::SetPause { paused } => crate::execute::set_pause(deps, info, paused),
            ExecuteMsg::WithdrawSurplus { to, amount } => {
                crate::execute::withdraw_surplus(deps, env, info, to, amount)
            }
            ExecuteMsg::SetRemoteOperators { attestors, quorum } => {
                crate::execute::set_remote_operators(deps, info, attestors, quorum)
            }
            ExecuteMsg::SetRemoteBinding { operator, domain, remote_address } => {
                crate::execute::set_remote_binding(deps, info, operator, domain, remote_address)
            }
            ExecuteMsg::SetRemoteReward { domain, reward } => {
                crate::execute::set_remote_reward(deps, info, domain, reward)
            }
            ExecuteMsg::SetOperatorAddress { index, domain, address } => {
                crate::execute::set_operator_address(deps, info, index, domain, address)
            }
            ExecuteMsg::SetRemoteRouter { domain, address } => {
                crate::execute::set_remote_router(deps, info, domain, address)
            }
            ExecuteMsg::AttestRemoteDelivery { domain, message_ids, executor } => {
                crate::execute::attest_remote_delivery(deps, env, info, domain, message_ids, executor)
            }
        }
    }

    #[entry_point]
    pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
        let res = match msg {
            QueryMsg::Config {} => to_json_binary(&crate::query::config(deps)?),
            QueryMsg::Claimed { message_id } => {
                to_json_binary(&crate::query::claimed(deps, message_id)?)
            }
            QueryMsg::Delivery { message_id } => {
                to_json_binary(&crate::query::delivery(deps, message_id)?)
            }
            QueryMsg::LayoutCheck { message_id } => {
                to_json_binary(&crate::query::layout_check(deps, message_id)?)
            }
            QueryMsg::Solvency {} => to_json_binary(&crate::query::solvency(deps, env)?),
            QueryMsg::RemoteConfig {} => to_json_binary(&crate::query::remote_config(deps)?),
            QueryMsg::RemoteBinding { operator, domain } => {
                to_json_binary(&crate::query::remote_binding(deps, operator, domain)?)
            }
            QueryMsg::RemoteReward { domain } => {
                to_json_binary(&crate::query::remote_reward(deps, domain)?)
            }
            QueryMsg::RemoteClaimed { message_id } => {
                to_json_binary(&crate::query::remote_claimed(deps, message_id)?)
            }
            QueryMsg::RemoteAttestations { message_id } => {
                to_json_binary(&crate::query::remote_attestations(deps, message_id)?)
            }
            QueryMsg::QuoteRemote { domain, message_ids } => {
                to_json_binary(&crate::query::quote_remote(deps, domain, message_ids)?)
            }
            QueryMsg::OperatorAddress { index, domain } => {
                to_json_binary(&crate::query::operator_address(deps, index, domain)?)
            }
            QueryMsg::OperatorOfLocal { address } => {
                to_json_binary(&crate::query::operator_of_local(deps, address)?)
            }
            QueryMsg::RemoteRouter { domain } => {
                to_json_binary(&crate::query::remote_router(deps, domain)?)
            }
        };
        Ok(res?)
    }

    #[entry_point]
    pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
        cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        Ok(Response::new().add_attribute("action", "migrate"))
    }
}
