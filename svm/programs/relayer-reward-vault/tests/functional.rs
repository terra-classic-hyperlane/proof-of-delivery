//! Testes funcionais do RelayerRewardVault com solana-program-test.

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
const REWARD: u64 = 1_000_000; // lamports por entrega
const NOW: i64 = 10_000_000; // → época corrente 10_000

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

    // relógio determinístico
    let mut clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = NOW;
    ctx.set_sysvar(&clock);

    let ops: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
    let (config, _) = config_pda(&program_id);

    // financia os operadores
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

    // semeia o pool (como o Claim do IGP faria)
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
    assert!(get_credit(&mut env, &op_a).await.is_none()); // abaixo do quórum

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
    let r2 = report(9_999, vec![(op_a, 99)]); // divergente

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_report_ix(&env, &s1, &r2);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();

    // 2 submissões, hashes diferentes → época NÃO fecha (travada p/ alarme)
    assert!(get_credit(&mut env, &op_a).await.is_none());
}

#[tokio::test]
async fn window_locked_by_first_submission() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r1 = report(9_999, vec![(op_a, 1)]);
    let mut r2 = r1.clone();
    r2.window_end_slot = 999; // janela diferente

    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_report_ix(&env, &s1, &r2);
    let err = send(&mut env.ctx, &[ix], &[s1]).await.unwrap_err();
    assert!(err.contains("0x66"), "esperava ERR_WINDOW_MISMATCH(102=0x66): {err}");
}

#[tokio::test]
async fn unsorted_credits_rejected() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let op_b = env.ops[1].pubkey();
    let mut credits = sorted_pair(op_a, 1, op_b, 1);
    credits.reverse(); // fora de ordem
    let r = report(9_999, credits);

    let s0 = env.ops[0].insecure_clone();
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x67"), "esperava ERR_UNSORTED(103=0x67): {err}");
}

#[tokio::test]
async fn open_epoch_rejected() {
    let mut env = setup(2).await;
    let op_a = env.ops[0].pubkey();
    let r = report(10_000, vec![(op_a, 1)]); // época CORRENTE (aberta)
    let s0 = env.ops[0].insecure_clone();
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x68"), "esperava ERR_EPOCH_OPEN(104=0x68): {err}");
}

#[tokio::test]
async fn non_operator_rejected() {
    let mut env = setup(2).await;
    let outsider = Keypair::new();
    transfer(&mut env.ctx, &outsider.pubkey(), 1_000_000_000).await;
    let r = report(9_999, vec![(outsider.pubkey(), 1)]);
    let ix = submit_report_ix(&env, &outsider, &r);
    let err = send(&mut env.ctx, &[ix], &[outsider]).await.unwrap_err();
    assert!(err.contains("0x64"), "esperava ERR_NOT_OPERATOR(100=0x64): {err}");
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

    // segundo saque acima do crédito falha
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
    assert!(err.contains("0x6b"), "esperava ERR_INSUFFICIENT_CREDIT(107=0x6b): {err}");
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
    assert!(!get_config(&mut env).await.paused); // 1 aprovação < quórum

    let ix = admin_ix(&s1);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();
    assert!(get_config(&mut env).await.paused); // quórum → executou

    // pausado bloqueia relatórios
    let op_a = env.ops[0].pubkey();
    let r = report(9_999, vec![(op_a, 1)]);
    let ix = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x65"), "esperava ERR_PAUSED(101=0x65): {err}");
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

    // o executor tenta REDIRECIONAR para outra conta → o hash trava
    let attacker_dest = Pubkey::new_unique();
    let ix = mk(&s1, attacker_dest);
    let err = send(&mut env.ctx, &[ix], &[s1.insecure_clone()]).await.unwrap_err();
    assert!(err.contains("0x6e"), "esperava ERR_BAD_DESTINATION(110=0x6e): {err}");

    // com o destino aprovado, executa
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
// v2 — créditos REMOTOS no relatório de época (ClaimRemote)
// ===========================================================================
use rrv::{remote_binding_pda, remote_reward_pda};

const DOM_TC: u32 = 132_556;
const RREWARD: u64 = 499_000;

/// aprova (quórum 1) uma AdminAction com contas extras
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

    // relatório SÓ com créditos remotos (2 entregas no TC)
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
async fn remote_sem_reward_ou_binding_rejeitado() {
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
    // sem reward PDA criado → ERR_NO_REMOTE_REWARD (113 = 0x71)
    let err = send(&mut env.ctx, &[ix], &[signer]).await.unwrap_err();
    assert!(err.contains("0x71"), "esperava ERR_NO_REMOTE_REWARD: {err}");
}

// ===========================================================================
// SEGURANÇA: guard de replay por bitmap no Config + close/refund do rent.
// (mudança de 2026-08-20; "dinheiro de terceiros" — foco em não pagar 2×)
// ===========================================================================

/// Após aplicar (quórum), a conta de época é FECHADA e o rent volta ao operador.
#[tokio::test]
async fn epoch_account_closed_and_rent_refunded_on_apply() {
    let mut env = setup(1).await; // quórum 1: cria+aplica+fecha numa tx
    let op_a = env.ops[0].pubkey();
    let (epoch_acc, _) = epoch_pda(&env.program_id, 9_999);
    let s0 = env.ops[0].insecure_clone();

    // 1ª época: cria o PDA de crédito (rent que PERSISTE — legítimo) + a conta de
    // época (rent que deve VOLTAR ao fechar).
    let r1 = report(9_998, vec![(op_a, 2)]);
    let __ix_a = submit_report_ix(&env, &s0, &r1);
    send(&mut env.ctx, &[__ix_a], &[s0.insecure_clone()]).await.unwrap();
    assert!(env.ctx.banks_client.get_account(epoch_pda(&env.program_id, 9_998).0).await.unwrap().is_none(),
        "a conta de época 1 deveria ter sido fechada");

    // 2ª época: o PDA de crédito JÁ existe → o único rent em jogo é o da conta de
    // época, que deve ser devolvido. A queda de saldo tem de ser só a fee (~5000).
    let bal_before = env.ctx.banks_client.get_balance(op_a).await.unwrap();
    let r2 = report(9_999, vec![(op_a, 2)]);
    let __ix_b = submit_report_ix(&env, &s0, &r2);
    send(&mut env.ctx, &[__ix_b], &[s0]).await.unwrap();

    assert!(env.ctx.banks_client.get_account(epoch_acc).await.unwrap().is_none(),
        "a conta de época 2 deveria ter sido fechada");
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 4 * REWARD);
    let bal_after = env.ctx.banks_client.get_balance(op_a).await.unwrap();
    // se o rent da época (~2,4M) tivesse ficado preso, a queda seria enorme.
    assert!(bal_before - bal_after < 50_000,
        "rent da época não foi devolvido: perdeu {} lamports", bal_before - bal_after);
}

/// CRÍTICO: re-submeter uma época JÁ PAGA é rejeitado — mesmo com a conta de
/// coleta já fechada. Sem duplo-pagamento.
#[tokio::test]
async fn replay_after_close_is_rejected() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    let r = report(9_999, vec![(op_a, 2)]);

    // 1ª aplicação: paga 2×REWARD e fecha a conta
    let __ix_1 = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[__ix_1], &[s0.insecure_clone()]).await.unwrap();
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);

    // 2ª submissão da MESMA época → ERR_APPLIED (105 = 0x69), NÃO paga de novo
    let r2 = report(9_999, vec![(op_a, 2)]);
    let __ix_100 = submit_report_ix(&env, &s0, &r2);
    let err = send(&mut env.ctx, &[__ix_100], &[s0]).await.unwrap_err();
    assert!(err.contains("0x69"), "esperava ERR_APPLIED no replay: {err}");
    // crédito inalterado (não dobrou)
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);
}

/// Época < base (fora da janela por trás) é rejeitada com ERR_EPOCH_TOO_OLD(115=0x73).
#[tokio::test]
async fn epoch_below_base_rejected() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    // base = 10_000 - 256 = 9_744; época 9_000 << base
    let r = report(9_000, vec![(op_a, 1)]);
    let __ix_101 = submit_report_ix(&env, &s0, &r);
    let err = send(&mut env.ctx, &[__ix_101], &[s0]).await.unwrap_err();
    assert!(err.contains("0x73"), "esperava ERR_EPOCH_TOO_OLD: {err}");
}

/// Fora-de-ordem DENTRO da janela é aceito (a vantagem sobre marca monotônica).
#[tokio::test]
async fn out_of_order_within_window_accepted() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();
    // aplica 9_998 e depois 9_990 (mais antiga, mas dentro da janela)
    let r_new = report(9_998, vec![(op_a, 1)]);
    let __ix_2 = submit_report_ix(&env, &s0, &r_new);
    send(&mut env.ctx, &[__ix_2], &[s0.insecure_clone()]).await.unwrap();
    let r_old = report(9_990, vec![(op_a, 1)]);
    let __ix_3 = submit_report_ix(&env, &s0, &r_old);
    send(&mut env.ctx, &[__ix_3], &[s0]).await.unwrap();
    // ambas creditadas
    assert_eq!(get_credit(&mut env, &op_a).await.unwrap().credited, 2 * REWARD);
}

/// SetAppliedBase é MONOTÔNICO: retroceder é rejeitado (117=0x75); avançar OK,
/// e o bitmap desliza preservando os bits (uma época recém-paga não reabre).
#[tokio::test]
async fn set_applied_base_is_monotonic_and_shifts_bitmap() {
    let mut env = setup(1).await;
    let op_a = env.ops[0].pubkey();
    let s0 = env.ops[0].insecure_clone();

    // paga a época 9_800 (base atual 9_744 → offset 56)
    let r = report(9_800, vec![(op_a, 1)]);
    let __ix_4 = submit_report_ix(&env, &s0, &r);
    send(&mut env.ctx, &[__ix_4], &[s0.insecure_clone()]).await.unwrap();

    // helper p/ admin action SetAppliedBase (captura campos p/ não emprestar env)
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

    // avança a base p/ 9_790 (desliza 46 bits): a época 9_800 continua marcada
    let ix = mk(&s0, 9_790);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();
    assert_eq!(get_config(&mut env).await.applied_base, 9_790);
    // re-submeter 9_800 ainda é replay (bit preservado no slide) → ERR_APPLIED
    let r2 = report(9_800, vec![(op_a, 1)]);
    let __ix_102 = submit_report_ix(&env, &s0, &r2);
    let err = send(&mut env.ctx, &[__ix_102], &[s0.insecure_clone()]).await.unwrap_err();
    assert!(err.contains("0x69"), "bit deveria sobreviver ao slide: {err}");

    // retroceder a base é REJEITADO (117 = 0x75)
    let ix = mk(&s0, 9_700);
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0x75"), "esperava ERR_BASE_NOT_MONOTONIC: {err}");
}
