use cosmwasm_std::{Deps, Env, HexBinary, Uint128};

use crate::error::ContractError;
use crate::mailbox::load_delivery;
use crate::msg::{
    ClaimedResponse, ConfigResponse, DeliveryResponse, LayoutCheckResponse, SolvencyResponse,
};
use crate::state::{CLAIMED, CONFIG, TOTAL_CLAIMS, TOTAL_PAID};

pub fn config(deps: Deps) -> Result<ConfigResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner,
        mailbox: config.mailbox,
        igp: config.igp,
        denom: config.denom,
        reward_per_delivery: config.reward_per_delivery,
        claim_window_blocks: config.claim_window_blocks,
        paused: config.paused,
        total_paid: TOTAL_PAID.load(deps.storage)?,
        total_claims: TOTAL_CLAIMS.load(deps.storage)?,
    })
}

pub fn claimed(deps: Deps, message_id: HexBinary) -> Result<ClaimedResponse, ContractError> {
    let record = CLAIMED.may_load(deps.storage, message_id.to_vec())?;
    Ok(match record {
        Some(r) => ClaimedResponse {
            claimed: true,
            claimant: Some(r.claimant),
            amount: Some(r.amount),
            claimed_at_block: Some(r.claimed_at_block),
        },
        None => ClaimedResponse {
            claimed: false,
            claimant: None,
            amount: None,
            claimed_at_block: None,
        },
    })
}

pub fn delivery(deps: Deps, message_id: HexBinary) -> Result<DeliveryResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let delivery = load_delivery(&deps.querier, &config.mailbox, message_id.as_slice())?;
    Ok(match delivery {
        Some(d) => DeliveryResponse {
            delivered: true,
            processor: Some(d.sender),
            delivered_at_block: Some(d.block_number),
        },
        None => DeliveryResponse {
            delivered: false,
            processor: None,
            delivered_at_block: None,
        },
    })
}

/// Sonda o layout: para uma mensagem SABIDAMENTE entregue, `ok=true` significa que
/// a chave existe e parseia estritamente. `ok=false` com o detalhe do erro é o
/// alarme de migrate no Mailbox (spec §06). Chave ausente também retorna ok=false
/// (mensagem errada ou layout de CHAVE alterado) — use um id confirmado.
pub fn layout_check(deps: Deps, message_id: HexBinary) -> Result<LayoutCheckResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    match load_delivery(&deps.querier, &config.mailbox, message_id.as_slice()) {
        Ok(Some(d)) => Ok(LayoutCheckResponse {
            ok: true,
            detail: format!(
                "delivery parsed: sender={}, block_number={}",
                d.sender, d.block_number
            ),
        }),
        Ok(None) => Ok(LayoutCheckResponse {
            ok: false,
            detail: "key not found — use a message id known to be delivered (or the key layout changed)".to_string(),
        }),
        Err(ContractError::MailboxLayoutMismatch { reason, .. }) => Ok(LayoutCheckResponse {
            ok: false,
            detail: format!("VALUE LAYOUT MISMATCH: {reason}"),
        }),
        Err(e) => Err(e),
    }
}

pub fn solvency(deps: Deps, env: Env) -> Result<SolvencyResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let pool = deps
        .querier
        .query_balance(&env.contract.address, &config.denom)?;
    let claims_payable = if config.reward_per_delivery.is_zero() {
        Uint128::zero()
    } else {
        pool.amount
            .checked_div(config.reward_per_delivery)
            .unwrap_or_default()
    };
    Ok(SolvencyResponse {
        pool,
        reward_per_delivery: config.reward_per_delivery,
        claims_payable,
    })
}
