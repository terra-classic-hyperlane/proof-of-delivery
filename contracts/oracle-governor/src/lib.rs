//! # oracle-governor
//!
//! Governor do StorageGasOracle (hpl-igp-oracle) no Terra Classic — spec §01/§10.
//!
//! Separação de poderes:
//! - A GOVERNANÇA (owner) define faixa [min,max] por domínio, variação máxima
//!   (bps), operadores e quórum — e mantém dois caminhos de emergência: escrita
//!   direta no oracle (`ForceSetRemoteGasData`) e devolução da posse
//!   (`InitOracleOwnershipTransfer`).
//! - Os OPERADORES apenas submetem o preço observado; ao atingir o quórum, a
//!   MEDIANA (menor dos centrais em empate par — na dúvida cobra menos do
//!   usuário) é validada contra faixa + delta e aplicada no oracle por CPI.
//!
//! O conflito de interesse (operadores controlam o preço que financia a própria
//! remuneração) é neutralizado porque a FAIXA é definida por quem não opera.

pub mod error;
pub mod execute;
pub mod msg;
pub mod oracle_api;
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

    pub const CONTRACT_NAME: &str = "crates.io:oracle-governor";
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
        use ExecuteMsg::*;
        match msg {
            SubmitPrice {
                domain,
                token_exchange_rate,
                gas_price,
            } => crate::execute::submit_price(deps, env, info, domain, token_exchange_rate, gas_price),
            SetBounds { domain, bounds } => crate::execute::set_bounds(deps, info, domain, bounds),
            UnsetBounds { domain } => crate::execute::unset_bounds(deps, info, domain),
            SetOperators { add, remove } => crate::execute::set_operators(deps, info, add, remove),
            SetQuorum { quorum } => crate::execute::set_quorum(deps, info, quorum),
            SetEpochDuration { epoch_duration_secs } => {
                crate::execute::set_epoch_duration(deps, info, epoch_duration_secs)
            }
            SetMaxDeltaBps { max_delta_bps } => {
                crate::execute::set_max_delta_bps(deps, info, max_delta_bps)
            }
            SetOwner { owner } => crate::execute::set_owner(deps, info, owner),
            ForceSetRemoteGasData {
                domain,
                token_exchange_rate,
                gas_price,
            } => crate::execute::force_set_remote_gas_data(
                deps,
                env,
                info,
                domain,
                token_exchange_rate,
                gas_price,
            ),
            InitOracleOwnershipTransfer { next_owner } => {
                crate::execute::init_oracle_ownership_transfer(deps, info, next_owner)
            }
            RevokeOracleOwnershipTransfer {} => {
                crate::execute::revoke_oracle_ownership_transfer(deps, info)
            }
            ClaimOracleOwnership {} => crate::execute::claim_oracle_ownership(deps, info),
        }
    }

    #[entry_point]
    pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
        let res = match msg {
            QueryMsg::Config {} => to_json_binary(&crate::query::config(deps)?),
            QueryMsg::Operators {} => to_json_binary(&crate::query::operators(deps)?),
            QueryMsg::Bounds { domain } => to_json_binary(&crate::query::bounds(deps, domain)?),
            QueryMsg::CurrentEpoch {} => to_json_binary(&crate::query::current_epoch(deps, env)?),
            QueryMsg::Submissions { domain, epoch } => {
                to_json_binary(&crate::query::submissions(deps, domain, epoch)?)
            }
            QueryMsg::Applied { domain, epoch } => {
                to_json_binary(&crate::query::applied(deps, domain, epoch)?)
            }
            QueryMsg::LastApplied { domain } => {
                to_json_binary(&crate::query::last_applied(deps, domain)?)
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
