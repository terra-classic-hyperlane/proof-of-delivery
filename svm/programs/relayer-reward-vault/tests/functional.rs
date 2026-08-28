//! Functional tests of the RelayerRewardVault with solana-program-test.

use borsh::BorshDeserialize;
use rrv::{
    config_pda, credit_pda, epoch_pda, proposal_pda, AdminAction, AdminEnvelope, Config,
    EpochReport, Instruction as RrvInstruction, OperatorCredit,
};
use solana_program::{
    clock::Clock, instruction::AccountMeta, instruction::Instruction, pubkey::Pubkey,
    system_program,
};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

const EPOCH_SECS: u64 = 1_000;
const REWARD: u64 = 1_000_000; // lamports per delivery
const NOW: i64 = 10_000_000; // → current epoch 10_000

struct Env {
    ctx: ProgramTestContext,
    program_id: Pubkey,
    config: Pubkey,
    ops: Vec<Keypair>,
}

async fn setup(quorum: u8) -> Env {
    let program_id = Pubkey::new_unique();
    let pt = ProgramTest::new("rrv", program_id, processor!(rrv::process_instruction));
    let mut ctx = pt.start_with_context().await;

    // deterministic clock
    let mut clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = NOW;
    ctx.set_sysvar(&clock);

    let ops: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
    let (config, _) = config_pda(&program_id);

    // funds the operators
    for op in &ops {
        transfer(&mut ctx, &op.pubkey(), 5_000_000_000).await;
    }

    // Init
    let data = borsh::to_vec(&RrvInstruction::Init {
        operators: ops.iter().map(|k| k.pubkey()).collect(),
        quorum,
        reward_lamports: REWARD,
        epoch_duration_secs: EPOCH_SECS,
    })
    .unwrap();
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(ctx.payer.pubkey(), true),
            AccountMeta::new(config, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    };
    send(&mut ctx, &[ix], &[]).await.unwrap();

    // seeds the pool (as the IGP Claim would)
    transfer(&mut ctx, &config, 100 * REWARD).await;

    Env {
        ctx,
        program_id,
        config,
        ops,
    }
}

async fn transfer(ctx: &mut ProgramTestContext, to: &Pubkey, lamports: u64) {
    let ix = system_instruction::transfer(&ctx.payer.pubkey(), to, lamports);
    let payer = ctx.payer.insecure_clone();
    send(ctx, &[ix], std::slice::from_ref(&payer)).await.unwrap();
}

async fn send(
    ctx: &mut ProgramTestContext,
    ixs: &[Instruction],
    extra_signers: &[Keypair],
) -> Result<(), String> {
    let blockhash = ctx
        .banks_client
        .get_latest_blockhash()
        .await
        .map_err(|e| e.to_string())?;
    let mut signers: Vec<&Keypair> = vec![&ctx.payer];
    for k in extra_signers {
        if k.pubkey() != ctx.payer.pubkey() {
            signers.push(k);
        }
    }
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&ctx.payer.pubkey()),
        &signers,
        blockhash,
    );
    ctx.banks_client
        .process_transaction(tx)
        .await
        .map_err(|e| e.to_string())
}

fn report(epoch: u64, credits: Vec<(Pubkey, u64)>) -> EpochReport {
    EpochReport {
        remote: vec![],
        epoch,
        window_start_slot: 100,
        window_end_slot: 200,
        credits,
    }
}

fn submit_report_ix(env: &Env, op: &Keypair, r: &EpochReport) -> Instruction {
    let (epoch_acc, _) = epoch_pda(&env.program_id, r.epoch);
    let mut accounts = vec![
        AccountMeta::new(op.pubkey(), true),
        AccountMeta::new(env.config, false),
        AccountMeta::new(epoch_acc, false),
        AccountMeta::new_readonly(system_program::id(), false),
    ];
    for (credited, _) in &r.credits {
        let (c, _) = credit_pda(&env.program_id, credited);
        accounts.push(AccountMeta::new(c, false));
    }
    Instruction {
        program_id: env.program_id,
        accounts,
        data: borsh::to_vec(&RrvInstruction::SubmitEpochReport { report: r.clone() }).unwrap(),
    }
}

async fn get_credit(env: &mut Env, op: &Pubkey) -> Option<OperatorCredit> {
    let (addr, _) = credit_pda(&env.program_id, op);
    let acc = env.ctx.banks_client.get_account(addr).await.unwrap()?;
    let mut slice: &[u8] = &acc.data;
    Some(OperatorCredit::deserialize(&mut slice).unwrap())
}

async fn get_config(env: &mut Env) -> Config {
    let acc = env
        .ctx
        .banks_client
        .get_account(env.config)
        .await
        .unwrap()
        .unwrap();
    let mut slice: &[u8] = &acc.data;
    Config::deserialize(&mut slice).unwrap()
}

fn sorted_pair(a: Pubkey, av: u64, b: Pubkey, bv: u64) -> Vec<(Pubkey, u64)> {
    let mut v = vec![(a, av), (b, bv)];
    v.sort_by_key(|(k, _)| *k);
    v
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn init_and_config() {
    let mut env = setup(2).await;
    let config = get_config(&mut env).await;
    assert_eq!(config.quorum, 2);
    assert_eq!(config.reward_lamports, REWARD);
    assert_eq!(config.operators.len(), 3);
    assert!(!config.paused);
}

#[tokio::test]
async fn quorum_applies_credits() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let op_b = env.ops[1].pubkey();
    let r = report(9_999, sorted_pair(op_a, 3, op_b, 1));

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    assert!(get_credit(&mut env, &op_a).await.is_none()); // below quorum

    let ix = submit_report_ix(&env, &s1, &r);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();

    let credit_a = get_credit(&mut env, &op_a).await.unwrap();
    let credit_b = get_credit(&mut env, &op_b).await.unwrap();
    assert_eq!(credit_a.credited, 3 * REWARD);
    assert_eq!(credit_b.credited, REWARD);
    assert_eq!(get_config(&mut env).await.total_credited, 4 * REWARD);
}

#[tokio::test]
async fn divergent_report_does_not_apply() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r1 = report(9_999, vec![(op_a, 3)]);
    let r2 = report(9_999, vec![(op_a, 99)]); // divergent

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_report_ix(&env, &s1, &r2);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();

    // 2 submissions, different hashes → epoch does NOT close (locked for alarm)
    assert!(get_credit(&mut env, &op_a).await.is_none());
}

#[tokio::test]
async fn window_locked_by_first_submission() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r1 = report(9_999, vec![(op_a, 1)]);
    let mut r2 = r1.clone();
    r2.window_end_slot = 999; // different window

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_report_ix(&env, &s1, &r2);
    let err = send(&mut env.ctx, &[ix], &[s1]).await.unwrap_err();
    assert!(err.contains("0x66"), "expected ERR_WINDOW_MISMATCH(102=0x66): {err}");
}

#[tokio::test]
async fn unsorted_credits_rejected() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let op_b = env.ops[1].pubkey();
    let mut credits = sorted_pair(op_a, 1, op_b, 1);
    credits.reverse(); // out of order
    let r = report(9_999, credits);

    let s0 = env.ops[0].insecure_clone();
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x67"), "expected ERR_UNSORTED(103=0x67): {err}");
}

#[tokio::test]
async fn open_epoch_rejected() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r = report(10_000, vec![(op_a, 1)]); // CURRENT epoch (open)
    let s0 = env.ops[0].insecure_clone();
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x68"), "expected ERR_EPOCH_OPEN(104=0x68): {err}");
}

#[tokio::test]
async fn non_operator_rejected() {
    let mut env = setup(2).await;
    let outsider = Keypair::new();
    transfer(&mut env.ctx, &outsider.pubkey(), 1_000_000_000).await;
    let r = report(9_999, vec![(outsider.pubkey(), 1)]);
    let ix = submit_report_ix(&env, &outsider, &r);
    let err = send(&mut env.ctx, &[ix], &[outsider]).await.unwrap_err();
    assert!(err.contains("0x64"), "expected ERR_NOT_OPERATOR(100=0x64): {err}");
}

#[tokio::test]
async fn withdraw_pays_and_respects_credit() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r = report(9_999, vec![(op_a, 2)]);
    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();
    let ix = submit_report_ix(&env, &s1, &r);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();

    let before = env
        .ctx
        .banks_client
        .get_account(op_a)
        .await
        .unwrap()
        .unwrap()
        .lamports;

    let (credit_acc, _) = credit_pda(&env.program_id, &op_a);
    let wd = Instruction {
        program_id: env.program_id,
        accounts: vec![
            AccountMeta::new(op_a, true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(credit_acc, false),
        ],
        data: borsh::to_vec(&RrvInstruction::Withdraw { amount: 2 * REWARD }).unwrap(),
    };
    send(&mut env.ctx, &[wd], &[s0.insecure_clone()]).await.unwrap();

    let after = env
        .ctx
        .banks_client
        .get_account(op_a)
        .await
        .unwrap()
        .unwrap()
        .lamports;
    assert_eq!(after - before, 2 * REWARD);

    // second withdrawal above the credit fails
    let wd2 = Instruction {
        program_id: env.program_id,
        accounts: vec![
            AccountMeta::new(op_a, true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(credit_acc, false),
        ],
        data: borsh::to_vec(&RrvInstruction::Withdraw { amount: 1 }).unwrap(),
    };
    let err = send(&mut env.ctx, &[wd2], &[s0]).await.unwrap_err();
    assert!(err.contains("0x6b"), "expected ERR_INSUFFICIENT_CREDIT(107=0x6b): {err}");
}

#[tokio::test]
async fn admin_proposal_pause_via_quorum() {
    let mut env = setup(2).await;
    let envelope = AdminEnvelope {
        nonce: 1,
        action: AdminAction::SetPaused(true),
    };
    let (prop, _) = proposal_pda(&env.program_id, &envelope.hash());

    let (program_id, config) = (env.program_id, env.config);
    let admin_ix = move |op: &Keypair| Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(op.pubkey(), true),
            AccountMeta::new(config, false),
            AccountMeta::new(prop, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&RrvInstruction::SubmitAdminAction {
            envelope: envelope.clone(),
        })
        .unwrap(),
    };

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = admin_ix(&s0);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();
    assert!(!get_config(&mut env).await.paused); // 1 approofl < quorum

    let ix = admin_ix(&s1);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();
    assert!(get_config(&mut env).await.paused); // quorum → executed

    // paused blocks reports
    let op_a = env.ops[0].pubkey();
    let r = report(9_999, vec![(op_a, 1)]);
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x65"), "expected ERR_PAUSED(101=0x65): {err}");
}

#[tokio::test]
async fn withdraw_surplus_enforces_destination_in_hash() {
    let mut env = setup(2).await;
    let treasury = Pubkey::new_unique();
    let envelope = AdminEnvelope {
        nonce: 7,
        action: AdminAction::WithdrawSurplus {
            to: treasury,
            amount: 10 * REWARD,
        },
    };
    let (prop, _) = proposal_pda(&env.program_id, &envelope.hash());

    let (program_id, config) = (env.program_id, env.config);
    let mk = move |op: &Keypair, destination: Pubkey| Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(op.pubkey(), true),
            AccountMeta::new(config, false),
            AccountMeta::new(prop, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(destination, false),
        ],
        data: borsh::to_vec(&RrvInstruction::SubmitAdminAction {
            envelope: envelope.clone(),
        })
        .unwrap(),
    };

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = mk(&s0, treasury);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();

    // the executor tries to REDIRECT to another account → the hash locks it
    let attacker_dest = Pubkey::new_unique();
    let ix = mk(&s1, attacker_dest);
    let err = send(&mut env.ctx, &[ix], &[s1.insecure_clone()]).await.unwrap_err();
    assert!(err.contains("0x6e"), "expected ERR_BAD_DESTINATION(110=0x6e): {err}");

    // with the approved destination, executes
    let ix = mk(&s1, treasury);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();
    let bal = env
        .ctx
        .banks_client
        .get_account(treasury)
        .await
        .unwrap()
        .unwrap()
        .lamports;
    assert_eq!(bal, 10 * REWARD);
}

// ===========================================================================
// v2 — REMOTE credits in the epoch report (ClaimRemote)
// ===========================================================================
use rrv::{remote_binding_pda, remote_reward_pda};

const DOM_TC: u32 = 132_556;
const RREWARD: u64 = 499_000;

/// approves (quorum 1) an AdminAction with extra accounts
async fn admin_exec(env: &mut Env, action: AdminAction, nonce: u64, extras: Vec<AccountMeta>) {
    let envelope = AdminEnvelope { nonce, action };
    let (prop, _) = proposal_pda(&env.program_id, &envelope.hash());
    let mut accounts = vec![
        AccountMeta::new(env.ops[0].pubkey(), true),
        AccountMeta::new(env.config, false),
        AccountMeta::new(prop, false),
        AccountMeta::new_readonly(system_program::id(), false),
    ];
    accounts.extend(extras);
    let ix = Instruction {
        program_id: env.program_id,
        accounts,
        data: borsh::to_vec(&RrvInstruction::SubmitAdminAction { envelope }).unwrap(),
    };
    let signer = env.ops[0].insecure_clone();
    send(&mut env.ctx, &[ix], &[signer]).await.unwrap();
}

#[tokio::test]
async fn remote_credits_via_epoch_report() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let (rw, _) = remote_reward_pda(&env.program_id, DOM_TC);
    let (bind, _) = remote_binding_pda(&env.program_id, DOM_TC, &op_a);

    admin_exec(
        &mut env,
        AdminAction::SetRemoteReward { domain: DOM_TC, reward: RREWARD },
        10,
        vec![AccountMeta::new(rw, false)],
    )
    .await;
    admin_exec(
        &mut env,
        AdminAction::SetRemoteBinding {
            domain: DOM_TC,
            operator: op_a,
            remote_address: "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp".into(),
        },
        11,
        vec![AccountMeta::new(bind, false)],
    )
    .await;

    // report with ONLY remote credits (2 deliveries on TC)
    let mut r = report(9_990, vec![]);
    r.remote = vec![(DOM_TC, op_a, 2)];
    let (epoch_acc, _) = epoch_pda(&env.program_id, 9_990);
    let (credit_a, _) = credit_pda(&env.program_id, &op_a);
    let signer = env.ops[0].insecure_clone();
    let ix = Instruction {
        program_id: env.program_id,
        accounts: vec![
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(epoch_acc, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(rw, false),
            AccountMeta::new_readonly(bind, false),
            AccountMeta::new(credit_a, false),
        ],
        data: borsh::to_vec(&RrvInstruction::SubmitEpochReport { report: r }).unwrap(),
    };
    send(&mut env.ctx, &[ix], &[signer]).await.unwrap();

    let credit = get_credit(&mut env, &op_a).await.unwrap();
    assert_eq!(credit.credited, 2 * RREWARD);
}

#[tokio::test]
async fn remote_without_reward_or_binding_rejected() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let (rw, _) = remote_reward_pda(&env.program_id, DOM_TC);
    let (bind, _) = remote_binding_pda(&env.program_id, DOM_TC, &op_a);
    let (epoch_acc, _) = epoch_pda(&env.program_id, 9_991);
    let (credit_a, _) = credit_pda(&env.program_id, &op_a);

    let mut r = report(9_991, vec![]);
    r.remote = vec![(DOM_TC, op_a, 1)];
    let signer = env.ops[0].insecure_clone();
    let ix = Instruction {
        program_id: env.program_id,
        accounts: vec![
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(epoch_acc, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(rw, false),
            AccountMeta::new_readonly(bind, false),
            AccountMeta::new(credit_a, false),
        ],
        data: borsh::to_vec(&RrvInstruction::SubmitEpochReport { report: r }).unwrap(),
    };
    // no reward PDA created → ERR_NO_REMOTE_REWARD (113 = 0x71)
    let err = send(&mut env.ctx, &[ix], &[signer]).await.unwrap_err();
    assert!(err.contains("0x71"), "expected ERR_NO_REMOTE_REWARD: {err}");
}

// ===========================================================================
// SECURITY: bitmap replay guard in Config + close/refund of rent.
// (change of 2026-08-20; "third-party money" — focus on not paying 2×)
// ===========================================================================

/// After applying (quorum), the epoch account is CLOSED and the rent returns to the operator.
#[tokio::test]
async fn epoch_account_closed_and_rent_refunded_on_apply() {
    let mut env = setup(1).await; // quorum 1: creates+applies+closes in one tx
    let op_a = env.ops[0].pubkey();
    let (epoch_acc, _) = epoch_pda(&env.program_id, 9_999);
    let s0 = env.ops[0].insecure_clone();

    // 1st epoch: creates the credit PDA (rent that PERSISTS — legitimate) + the epoch
    // account (rent that must RETURN on close).
    let r1 = report(9_998, vec![(op_a, 2)]);
    let __ix_a = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[__ix_a], &[s0.insecure_clone()]).await.unwrap();
    assert!(env.ctx.banks_client.get_account(epoch_pda(&env.program_id, 9_998).0).await.unwrap().is_none(),
        "epoch 1 account should have been closed");

    // 2nd epoch: the credit PDA ALREADY exists → the only rent at stake is that of the epoch
    // account, which must be refunded. The balance drop must be only the fee (~5000).
    let bal_before = env.ctx.banks_client.get_balance(op_a).await.unwrap();
    let r2 = report(9_999, vec![(op_a, 2)]);
    let __ix_b = submit_report_ix(&env, &s0, &r2);
    send(&mut env.ctx, &[__ix_b], &[s0]).await.unwrap();

    assert!(env.ctx.banks_client.get_account(epoch_acc).await.unwrap().is_none(),
        "epoch 2 account should have been closed");
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 4 * REWARD);
    let bal_after = env.ctx.banks_client.get_balance(op_a).await.unwrap();
    // if the epoch rent (~2.4M) had stayed stuck, the drop would be enormous.
    assert!(bal_before - bal_after < 50_000,
        "epoch rent was not refunded: lost {} lamports", bal_before - bal_after);
}

/// CRITICAL: re-submitting an ALREADY-PAID epoch is rejected — even with the
/// collection account already closed. No double-payment.
#[tokio::test]
async fn replay_after_close_is_rejected() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    let r = report(9_999, vec![(op_a, 2)]);

    // 1st application: pays 2×REWARD and closes the account
    let __ix_1 = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[__ix_1], &[s0.insecure_clone()]).await.unwrap();
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);

    // 2nd submission of the SAME epoch → ERR_APPLIED (105 = 0x69), does NOT pay again
    let r2 = report(9_999, vec![(op_a, 2)]);
    let __ix_100 = submit_report_ix(&env, &s0, &r2);
    let err = send(&mut env.ctx, &[__ix_100], &[s0]).await.unwrap_err();
    assert!(err.contains("0x69"), "expected ERR_APPLIED on replay: {err}");
    // credit unchanged (did not double)
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);
}

/// Epoch < base (out of the window behind) is rejected with ERR_EPOCH_TOO_OLD(115=0x73).
#[tokio::test]
async fn epoch_below_base_rejected() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    // base = 10_000 - 256 = 9_744; epoch 9_000 << base
    let r = report(9_000, vec![(op_a, 1)]);
    let __ix_101 = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[__ix_101], &[s0]).await.unwrap_err();
    assert!(err.contains("0x73"), "expected ERR_EPOCH_TOO_OLD: {err}");
}

/// Out-of-order WITHIN the window is accepted (the advantage over a monotonic marker).
#[tokio::test]
async fn out_of_order_within_window_accepted() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    // applies 9_998 and then 9_990 (older, but within the window)
    let r_new = report(9_998, vec![(op_a, 1)]);
    let __ix_2 = submit_report_ix(&env, &s0, &r_new);
    send(&mut env.ctx, &[__ix_2], &[s0.insecure_clone()]).await.unwrap();
    let r_old = report(9_990, vec![(op_a, 1)]);
    let __ix_3 = submit_report_ix(&env, &s0, &r_old);
    send(&mut env.ctx, &[__ix_3], &[s0]).await.unwrap();
    // both credited
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);
}

/// SetAppliedBase is MONOTONIC: going back is rejected (117=0x75); advancing is OK,
/// and the bitmap slides preserving the bits (a just-paid epoch does not reopen).
#[tokio::test]
async fn set_applied_base_is_monotonic_and_shifts_bitmap() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();

    // pays epoch 9_800 (current base 9_744 → offset 56)
    let r = report(9_800, vec![(op_a, 1)]);
    let __ix_4 = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[__ix_4], &[s0.insecure_clone()]).await.unwrap();

    // helper for the SetAppliedBase admin action (captures fields so as not to borrow env)
    let (program_id, config) = (env.program_id, env.config);
    let mk = move |op: &Keypair, base: u64| {
        let envelope = AdminEnvelope { nonce: base, action: AdminAction::SetAppliedBase(base) };
        let (prop, _) = proposal_pda(&program_id, &envelope.hash());
        Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(op.pubkey(), true),
                AccountMeta::new(config, false),
                AccountMeta::new(prop, false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data: borsh::to_vec(&RrvInstruction::SubmitAdminAction { envelope }).unwrap(),
        }
    };

    // advances the base to 9_790 (slides 46 bits): epoch 9_800 stays marked
    let ix = mk(&s0, 9_790);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();
    assert_eq!(get_config(&mut env).await.applied_base, 9_790);
    // re-submitting 9_800 is still a replay (bit preserved in the slide) → ERR_APPLIED
    let r2 = report(9_800, vec![(op_a, 1)]);
    let __ix_102 = submit_report_ix(&env, &s0, &r2);
    let err = send(&mut env.ctx, &[__ix_102], &[s0.insecure_clone()]).await.unwrap_err();
    assert!(err.contains("0x69"), "bit should survive the slide: {err}");

    // going back on the base is REJECTED (117 = 0x75)
    let ix = mk(&s0, 9_700);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x75"), "expected ERR_BASE_NOT_MONOTONIC: {err}");
}
