use std::collections::BTreeSet;

use cosmwasm_std::{
    ensure, to_json_binary, BankMsg, Coin, DepsMut, Env, HexBinary, MessageInfo, Response, Uint128,
    WasmMsg,
};

use crate::error::ContractError;
use crate::mailbox::{hex, load_delivery};
use crate::msg::InstantiateMsg;
use crate::state::{
    ClaimRecord, Config, RemoteClaimRecord, RemoteConfig, CLAIMED, CONFIG, REMOTE_ATTESTS,
    REMOTE_BINDINGS, REMOTE_CLAIMED, REMOTE_CONFIG, REMOTE_REWARDS, TOTAL_CLAIMS, TOTAL_PAID,
    TOTAL_REMOTE_PAID,
};

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(!msg.reward_per_delivery.is_zero(), ContractError::ZeroReward {});
    ensure!(msg.claim_window_blocks > 0, ContractError::ZeroWindow {});

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        mailbox: deps.api.addr_validate(&msg.mailbox)?,
        igp: deps.api.addr_validate(&msg.igp)?,
        denom: msg.denom,
        reward_per_delivery: msg.reward_per_delivery,
        claim_window_blocks: msg.claim_window_blocks,
        paused: false,
    };
    CONFIG.save(deps.storage, &config)?;
    TOTAL_PAID.save(deps.storage, &Uint128::zero())?;
    TOTAL_CLAIMS.save(deps.storage, &0u64)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", config.owner)
        .add_attribute("mailbox", config.mailbox)
        .add_attribute("igp", config.igp))
}

/// Resgate em lote, ATÔMICO. Para cada id: prova por raw query no Mailbox,
/// autoria (`sender == info.sender`), janela, e não-duplicidade. Grava o registro
/// (effects) ANTES do BankMsg e checa a solvência do pool contra o total do lote.
pub fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    message_ids: Vec<HexBinary>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(!config.paused, ContractError::Paused {});
    ensure!(!message_ids.is_empty(), ContractError::EmptyBatch {});

    // duplicata dentro do próprio lote também é rejeitada (o segundo save passaria
    // pelo CLAIMED.has feito antes — por isso o guard explícito).
    let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();

    let mut events_ids: Vec<String> = Vec::with_capacity(message_ids.len());
    for raw_id in &message_ids {
        let id = raw_id.as_slice();
        ensure!(
            id.len() == 32,
            ContractError::InvalidMessageId { len: id.len() }
        );
        ensure!(
            seen.insert(id.to_vec()),
            ContractError::DuplicatedId { id: hex(id) }
        );

        if let Some(previous) = CLAIMED.may_load(deps.storage, id.to_vec())? {
            return Err(ContractError::AlreadyClaimed {
                id: hex(id),
                claimant: previous.claimant,
            });
        }

        let delivery = load_delivery(&deps.querier, &config.mailbox, id)?
            .ok_or_else(|| ContractError::NotDelivered { id: hex(id) })?;

        ensure!(
            delivery.sender == info.sender,
            ContractError::NotProcessor {
                id: hex(id),
                processor: delivery.sender,
            }
        );

        let deadline = delivery.block_number.saturating_add(config.claim_window_blocks);
        ensure!(
            env.block.height <= deadline,
            ContractError::ClaimWindowExpired {
                id: hex(id),
                delivered_at: delivery.block_number,
                deadline,
                current: env.block.height,
            }
        );

        CLAIMED.save(
            deps.storage,
            id.to_vec(),
            &ClaimRecord {
                claimant: info.sender.clone(),
                amount: config.reward_per_delivery,
                claimed_at_block: env.block.height,
            },
        )?;
        events_ids.push(hex(id));
    }

    let count = Uint128::from(message_ids.len() as u128);
    let total = config
        .reward_per_delivery
        .checked_mul(count)
        .map_err(cosmwasm_std::StdError::overflow)?;

    // Solvência: o pool (saldo próprio) precisa cobrir o lote inteiro. O Sweep do
    // IGP pode ir na MESMA transação, antes do Claim, para engordar o saldo.
    let pool = deps
        .querier
        .query_balance(&env.contract.address, &config.denom)?;
    ensure!(
        pool.amount >= total,
        ContractError::InsufficientPool {
            needed: total.to_string(),
            available: pool.amount.to_string(),
            denom: config.denom.clone(),
        }
    );

    TOTAL_PAID.update(deps.storage, |paid| -> Result<_, ContractError> {
        Ok(paid.checked_add(total).map_err(cosmwasm_std::StdError::overflow)?)
    })?;
    TOTAL_CLAIMS.update(deps.storage, |n| -> Result<_, ContractError> {
        Ok(n + message_ids.len() as u64)
    })?;

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![Coin {
                denom: config.denom,
                amount: total,
            }],
        })
        .add_attribute("action", "claim")
        .add_attribute("claimant", info.sender)
        .add_attribute("count", message_ids.len().to_string())
        .add_attribute("total", total)
        .add_attribute("message_ids", events_ids.join(",")))
}

/// Permissionless: manda o vault chamar `claim()` no IGP. O IGP exige que o caller
/// seja o beneficiary — que é ESTE contrato — e transfere todo o saldo para cá.
pub fn sweep(deps: DepsMut, _env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // hpl_interface::igp::core::ExecuteMsg::Claim {} → {"claim":{}}
    #[derive(serde::Serialize)]
    #[serde(rename_all = "snake_case")]
    enum IgpExecuteMsg {
        Claim {},
    }

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: config.igp.to_string(),
            msg: to_json_binary(&IgpExecuteMsg::Claim {})?,
            funds: vec![],
        })
        .add_attribute("action", "sweep")
        .add_attribute("igp", config.igp))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    mailbox: Option<String>,
    igp: Option<String>,
    reward_per_delivery: Option<Uint128>,
    claim_window_blocks: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    if let Some(owner) = owner {
        config.owner = deps.api.addr_validate(&owner)?;
    }
    if let Some(mailbox) = mailbox {
        config.mailbox = deps.api.addr_validate(&mailbox)?;
    }
    if let Some(igp) = igp {
        config.igp = deps.api.addr_validate(&igp)?;
    }
    if let Some(reward) = reward_per_delivery {
        ensure!(!reward.is_zero(), ContractError::ZeroReward {});
        config.reward_per_delivery = reward;
    }
    if let Some(window) = claim_window_blocks {
        ensure!(window > 0, ContractError::ZeroWindow {});
        config.claim_window_blocks = window;
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("owner", config.owner)
        .add_attribute("reward_per_delivery", config.reward_per_delivery)
        .add_attribute("claim_window_blocks", config.claim_window_blocks.to_string()))
}

pub fn set_pause(
    deps: DepsMut,
    info: MessageInfo,
    paused: bool,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    config.paused = paused;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "set_pause")
        .add_attribute("paused", paused.to_string()))
}

pub fn withdraw_surplus(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    to: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    ensure!(!amount.is_zero(), ContractError::ZeroWithdraw {});
    let to = deps.api.addr_validate(&to)?;

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: to.to_string(),
            amount: vec![Coin {
                denom: config.denom,
                amount,
            }],
        })
        .add_attribute("action", "withdraw_surplus")
        .add_attribute("to", to)
        .add_attribute("amount", amount))
}

// ---------------------------------------------------------------------------
// v2 — ClaimRemote (taxa de origem por entrega remota atestada)
// ---------------------------------------------------------------------------

pub fn set_remote_operators(
    deps: DepsMut,
    info: MessageInfo,
    attestors: Vec<String>,
    quorum: u32,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    let attestors: Vec<_> = attestors
        .iter()
        .map(|a| deps.api.addr_validate(a))
        .collect::<Result<_, _>>()?;
    ensure!(
        quorum >= 1 && (quorum as usize) <= attestors.len(),
        ContractError::BadRemoteQuorum {}
    );
    REMOTE_CONFIG.save(deps.storage, &RemoteConfig { attestors, quorum })?;
    Ok(Response::new()
        .add_attribute("action", "set_remote_operators")
        .add_attribute("quorum", quorum.to_string()))
}

pub fn set_remote_binding(
    deps: DepsMut,
    info: MessageInfo,
    operator: String,
    domain: u32,
    remote_address: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    let operator = deps.api.addr_validate(&operator)?;
    match &remote_address {
        Some(addr) => {
            ensure!(
                !addr.is_empty() && addr.len() <= 100,
                ContractError::Std(cosmwasm_std::StdError::generic_err("invalid remote address"))
            );
            // EVM: normaliza p/ minúsculo; base58 (Solana) é case-sensitive
            let normalized = if addr.starts_with("0x") { addr.to_lowercase() } else { addr.clone() };
            REMOTE_BINDINGS.save(deps.storage, (&operator, domain), &normalized)?;
        }
        None => REMOTE_BINDINGS.remove(deps.storage, (&operator, domain)),
    }
    Ok(Response::new()
        .add_attribute("action", "set_remote_binding")
        .add_attribute("operator", operator)
        .add_attribute("domain", domain.to_string())
        .add_attribute("remote_address", remote_address.unwrap_or_else(|| "(removed)".into())))
}

pub fn set_remote_reward(
    deps: DepsMut,
    info: MessageInfo,
    domain: u32,
    reward: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    if reward.is_zero() {
        REMOTE_REWARDS.remove(deps.storage, domain);
    } else {
        REMOTE_REWARDS.save(deps.storage, domain, &reward)?;
    }
    Ok(Response::new()
        .add_attribute("action", "set_remote_reward")
        .add_attribute("domain", domain.to_string())
        .add_attribute("reward", reward))
}

/// Atesta entregas remotas e paga quando o quórum concorda. ATÔMICO como o
/// `Claim`: id inválido/duplicado/já pago reverte o lote inteiro.
pub fn attest_remote_delivery(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    domain: u32,
    message_ids: Vec<HexBinary>,
    executor: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(!config.paused, ContractError::Paused {});
    ensure!(!message_ids.is_empty(), ContractError::EmptyBatch {});

    let rc = REMOTE_CONFIG.may_load(deps.storage)?.unwrap_or_default();
    ensure!(rc.attestors.contains(&info.sender), ContractError::NotAttestor {});

    let executor = match executor {
        Some(a) => deps.api.addr_validate(&a)?,
        None => info.sender.clone(),
    };
    // o executor precisa de vínculo p/ o domínio — é o elo de identidade
    REMOTE_BINDINGS
        .may_load(deps.storage, (&executor, domain))?
        .ok_or_else(|| ContractError::NoBinding {
            operator: executor.to_string(),
            domain,
        })?;
    let reward = REMOTE_REWARDS
        .may_load(deps.storage, domain)?
        .filter(|r| !r.is_zero())
        .ok_or(ContractError::NoRemoteReward { domain })?;

    let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut paid: Vec<String> = vec![];
    let mut pending: Vec<String> = vec![];
    for raw_id in &message_ids {
        let id = raw_id.as_slice();
        ensure!(id.len() == 32, ContractError::InvalidMessageId { len: id.len() });
        ensure!(seen.insert(id.to_vec()), ContractError::DuplicatedId { id: hex(id) });

        if let Some(prev) = REMOTE_CLAIMED.may_load(deps.storage, id.to_vec())? {
            return Err(ContractError::RemoteAlreadyClaimed {
                id: hex(id),
                executor: prev.executor.to_string(),
            });
        }
        let mut atts = REMOTE_ATTESTS
            .may_load(deps.storage, id.to_vec())?
            .unwrap_or_default();
        ensure!(
            !atts.iter().any(|(a, _)| *a == info.sender),
            ContractError::AlreadyAttested { id: hex(id) }
        );
        atts.push((info.sender.clone(), executor.clone()));

        // ANTI-AUTOPAGAMENTO: com quórum >= 2, atestações onde o atestador é o
        // PRÓPRIO beneficiário não contam — exige `quorum` operadores independentes.
        let agree = atts
            .iter()
            .filter(|(a, e)| *e == executor && !(rc.quorum >= 2 && *a == executor))
            .count() as u32;
        if agree >= rc.quorum {
            // effects-first: marca pago ANTES do BankMsg
            REMOTE_CLAIMED.save(
                deps.storage,
                id.to_vec(),
                &RemoteClaimRecord {
                    executor: executor.clone(),
                    domain,
                    amount: reward,
                    claimed_at_block: env.block.height,
                },
            )?;
            REMOTE_ATTESTS.remove(deps.storage, id.to_vec());
            paid.push(hex(id));
        } else {
            REMOTE_ATTESTS.save(deps.storage, id.to_vec(), &atts)?;
            pending.push(hex(id));
        }
    }

    let mut resp = Response::new()
        .add_attribute("action", "attest_remote_delivery")
        .add_attribute("domain", domain.to_string())
        .add_attribute("attestor", info.sender.clone())
        .add_attribute("executor", executor.clone())
        .add_attribute("paid", paid.len().to_string())
        .add_attribute("pending_quorum", pending.len().to_string());
    if !pending.is_empty() {
        resp = resp.add_attribute("pending_ids", pending.join(","));
    }

    if !paid.is_empty() {
        let total = reward
            .checked_mul(Uint128::from(paid.len() as u128))
            .map_err(cosmwasm_std::StdError::overflow)?;
        let pool = deps
            .querier
            .query_balance(&env.contract.address, &config.denom)?;
        ensure!(
            pool.amount >= total,
            ContractError::InsufficientPool {
                needed: total.to_string(),
                available: pool.amount.to_string(),
                denom: config.denom.clone(),
            }
        );
        let cur = TOTAL_REMOTE_PAID.may_load(deps.storage)?.unwrap_or_default();
        TOTAL_REMOTE_PAID.save(
            deps.storage,
            &cur.checked_add(total).map_err(cosmwasm_std::StdError::overflow)?,
        )?;
        resp = resp
            .add_message(BankMsg::Send {
                to_address: executor.to_string(),
                amount: vec![Coin { denom: config.denom, amount: total }],
            })
            .add_attribute("total", total)
            .add_attribute("paid_ids", paid.join(","));
    }
    Ok(resp)
}
