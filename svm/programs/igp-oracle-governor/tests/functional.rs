//! Testes funcionais do IgpOracleGovernor com o mock do IGP (mesmos índices
//! borsh e layout de contas do programa real).

use borsh::BorshDeserialize;
use igp_oracle_governor::{
    config_pda, domain_pda, price_round_pda, Bounds, Instruction as GovInstruction,
};
use mock_igp::MockIgpState;
use solana_program::{
    clock::Clock, instruction::AccountMeta, instruction::Instruction, pubkey::Pubkey,
    system_program,
};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

const DOMAIN: u32 = 1399811149; // solana mainnet domain (exemplo)
const EPOCH_SECS: u64 = 21_600;
const DELTA_BPS: u64 = 2_000;
const NOW: i64 = 1_700_000_000;
const DECIMALS: u8 = 9;

struct Env {
    ctx: ProgramTestContext,
    gov_id: Pubkey,
    igp_id: Pubkey,
    igp_account: Pubkey,
    config: Pubkey,
    multisig: Keypair,
    ops: Vec<Keypair>,
}

async fn setup() -> Env {
    let gov_id = Pubkey::new_unique();
    let igp_id = Pubkey::new_unique();
    let igp_account = Pubkey::new_unique();

    let (config, _) = config_pda(&gov_id);

    let mut pt = ProgramTest::new(
        "igp_oracle_governor",
        gov_id,
        processor!(igp_oracle_governor::process_instruction),
    );
    pt.add_program("mock_igp", igp_id, processor!(mock_igp::process_instruction));

    // conta do IGP pré-populada: owner = config PDA do governor
    let state = MockIgpState {
        owner: Some(config),
        beneficiary: Pubkey::new_unique(),
        oracles: vec![],
    };
    let mut data = borsh::to_vec(&state).unwrap();
    data.resize(1024, 0);
    pt.add_account(
        igp_account,
        Account {
            lamports: 10_000_000,
            data,
            owner: igp_id,
            executable: false,
            rent_epoch: 0,
        },
    );

    let mut ctx = pt.start_with_context().await;
    let mut clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = NOW;
    ctx.set_sysvar(&clock);

    let multisig = Keypair::new();
    let ops: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
    for k in ops.iter().chain(std::iter::once(&multisig)) {
        let ix = system_instruction::transfer(&ctx.payer.pubkey(), &k.pubkey(), 5_000_000_000);
        let payer = ctx.payer.insecure_clone();
        send(&mut ctx, &[ix], &[payer]).await.unwrap();
    }

    // Init
    let data = borsh::to_vec(&GovInstruction::Init {
        multisig: multisig.pubkey(),
        operators: ops.iter().map(|k| k.pubkey()).collect(),
        quorum: 2,
        epoch_duration_secs: EPOCH_SECS,
        max_delta_bps: DELTA_BPS,
        igp_program: igp_id,
        igp: igp_account,
    })
    .unwrap();
    let ix = Instruction {
        program_id: gov_id,
        accounts: vec![
            AccountMeta::new(ctx.payer.pubkey(), true),
            AccountMeta::new(config, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    };
    send(&mut ctx, &[ix], &[]).await.unwrap();

    let mut env = Env {
        ctx,
        gov_id,
        igp_id,
        igp_account,
        config,
        multisig,
        ops,
    };

    // faixa do multisig para o domínio de teste
    set_domain(&mut env, 10, 1_000, 1, 10_000).await.unwrap();
    env
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
    let tx = Transaction::new_signed_with_payer(ixs, Some(&ctx.payer.pubkey()), &signers, blockhash);
    ctx.banks_client
        .process_transaction(tx)
        .await
        .map_err(|e| e.to_string())
}

async fn set_domain(
    env: &mut Env,
    min_rate: u128,
    max_rate: u128,
    min_gas: u128,
    max_gas: u128,
) -> Result<(), String> {
    let (domain_acc, _) = domain_pda(&env.gov_id, DOMAIN);
    let ix = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(env.multisig.pubkey(), true),
            AccountMeta::new_readonly(env.config, false),
            AccountMeta::new(domain_acc, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&GovInstruction::SetDomainConfig {
            domain: DOMAIN,
            bounds: Bounds {
                min_exchange_rate: min_rate,
                max_exchange_rate: max_rate,
                min_gas_price: min_gas,
                max_gas_price: max_gas,
            },
            token_decimals: DECIMALS,
        })
        .unwrap(),
    };
    let signer = env.multisig.insecure_clone();
    send(&mut env.ctx, &[ix], &[signer]).await
}

fn submit_ix(env: &Env, op: &Keypair, _epoch: u64, rate: u128, gas: u128) -> Instruction {
    let (domain_acc, _) = domain_pda(&env.gov_id, DOMAIN);
    let (round, _) = price_round_pda(&env.gov_id, DOMAIN);
    Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(op.pubkey(), true),
            AccountMeta::new_readonly(env.config, false),
            AccountMeta::new(domain_acc, false),
            AccountMeta::new(round, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(env.igp_id, false),
            AccountMeta::new(env.igp_account, false),
        ],
        data: borsh::to_vec(&GovInstruction::SubmitPrice {
            domain: DOMAIN,
            token_exchange_rate: rate,
            gas_price: gas,
        })
        .unwrap(),
    }
}

fn epoch_now() -> u64 {
    NOW as u64 / EPOCH_SECS
}

async fn igp_state(env: &mut Env) -> MockIgpState {
    let acc = env
        .ctx
        .banks_client
        .get_account(env.igp_account)
        .await
        .unwrap()
        .unwrap();
    let mut slice: &[u8] = &acc.data;
    MockIgpState::deserialize(&mut slice).unwrap()
}

async fn warp_epoch(env: &mut Env, epochs: u64) {
    let mut clock: Clock = env.ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp += (epochs * EPOCH_SECS) as i64;
    env.ctx.set_sysvar(&clock);
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn quorum_applies_median_via_cpi() {
    let mut env = setup().await;
    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());

    let ix = submit_ix(&env, &s0, epoch_now(), 100, 10);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    assert!(igp_state(&mut env).await.oracles.is_empty()); // abaixo do quórum

    let ix = submit_ix(&env, &s1, epoch_now(), 200, 40);
    send(&mut env.ctx, &[ix], &[s1]).await.unwrap();

    // par: menor dos centrais → 100/10 · token_decimals vem do domínio (multisig)
    let state = igp_state(&mut env).await;
    assert_eq!(state.oracles.len(), 1);
    let (domain, data) = &state.oracles[0];
    assert_eq!(*domain, DOMAIN);
    assert_eq!(data.token_exchange_rate, 100);
    assert_eq!(data.gas_price, 10);
    assert_eq!(data.token_decimals, DECIMALS);
}

#[tokio::test]
async fn non_operator_and_out_of_bounds_rejected() {
    let mut env = setup().await;

    let outsider = Keypair::new();
    let ix = system_instruction::transfer(&env.ctx.payer.pubkey(), &outsider.pubkey(), 1_000_000_000);
    let payer = env.ctx.payer.insecure_clone();
    send(&mut env.ctx, &[ix], &[payer]).await.unwrap();
    let ix = submit_ix(&env, &outsider, epoch_now(), 100, 10);
    let err = send(&mut env.ctx, &[ix], &[outsider]).await.unwrap_err();
    assert!(err.contains("0xc8"), "esperava ERR_NOT_OPERATOR(200=0xc8): {err}");

    let s0 = env.ops[0].insecure_clone();
    let ix = submit_ix(&env, &s0, epoch_now(), 5_000, 10); // rate > max 1000
    let err = send(&mut env.ctx, &[ix], &[s0]).await.unwrap_err();
    assert!(err.contains("0xcb"), "esperava ERR_OUT_OF_BOUNDS(203=0xcb): {err}");
}

#[tokio::test]
async fn delta_blocks_and_epoch_locks() {
    let mut env = setup().await;
    let (s0, s1, s2) = (
        env.ops[0].insecure_clone(),
        env.ops[1].insecure_clone(),
        env.ops[2].insecure_clone(),
    );

    // época 1: base 100
    let ix = submit_ix(&env, &s0, epoch_now(), 100, 100);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();
    let ix = submit_ix(&env, &s1, epoch_now(), 100, 100);
    send(&mut env.ctx, &[ix], &[s1.insecure_clone()]).await.unwrap();

    // época travada: 3ª submissão na mesma época falha
    let ix = submit_ix(&env, &s2, epoch_now(), 100, 100);
    let err = send(&mut env.ctx, &[ix], &[s2]).await.unwrap_err();
    assert!(err.contains("0xcc"), "esperava ERR_APPLIED(204=0xcc): {err}");

    // época 2: salto de 30% > 20% → bloqueia
    warp_epoch(&mut env, 1).await;
    let e2 = epoch_now() + 1;
    let ix = submit_ix(&env, &s0, e2, 130, 100);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_ix(&env, &s1, e2, 130, 100);
    let err = send(&mut env.ctx, &[ix], &[s1]).await.unwrap_err();
    assert!(err.contains("0xcd"), "esperava ERR_DELTA(205=0xcd): {err}");
}

#[tokio::test]
async fn close_round_protects_live_account() {
    let mut env = setup().await;
    let s0 = env.ops[0].insecure_clone();
    // cria a conta viva (única por domínio)
    let ix = submit_ix(&env, &s0, epoch_now(), 100, 100);
    send(&mut env.ctx, &[ix], &[s0.insecure_clone()]).await.unwrap();

    // CloseRound na conta VIVA deve falhar (ERR_ROUND_LIVE = 209 = 0xd1)
    let (live, _) = price_round_pda(&env.gov_id, DOMAIN);
    let close = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(s0.pubkey(), true),
            AccountMeta::new_readonly(env.config, false),
            AccountMeta::new(live, false),
        ],
        data: borsh::to_vec(&GovInstruction::CloseRound).unwrap(),
    };
    let err = send(&mut env.ctx, &[close], &[s0]).await.unwrap_err();
    assert!(err.contains("0xd1"), "esperava ERR_ROUND_LIVE(209=0xd1): {err}");
}

#[tokio::test]
async fn force_set_and_beneficiary_by_multisig_only() {
    let mut env = setup().await;
    let (domain_acc, _) = domain_pda(&env.gov_id, DOMAIN);

    // não-multisig falha
    let s0 = env.ops[0].insecure_clone();
    let bad = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(s0.pubkey(), true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(domain_acc, false),
            AccountMeta::new_readonly(env.igp_id, false),
            AccountMeta::new(env.igp_account, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&GovInstruction::ForceSetGasData {
            domain: DOMAIN,
            token_exchange_rate: 500,
            gas_price: 700,
        })
        .unwrap(),
    };
    let err = send(&mut env.ctx, &[bad], &[s0]).await.unwrap_err();
    assert!(err.contains("0xc9"), "esperava ERR_NOT_MULTISIG(201=0xc9): {err}");

    // multisig: força preço direto no IGP
    let multisig = env.multisig.insecure_clone();
    let ix = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(multisig.pubkey(), true),
            AccountMeta::new(env.config, false),
            AccountMeta::new(domain_acc, false),
            AccountMeta::new_readonly(env.igp_id, false),
            AccountMeta::new(env.igp_account, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: borsh::to_vec(&GovInstruction::ForceSetGasData {
            domain: DOMAIN,
            token_exchange_rate: 500,
            gas_price: 700,
        })
        .unwrap(),
    };
    send(&mut env.ctx, &[ix], &[multisig.insecure_clone()]).await.unwrap();
    let state = igp_state(&mut env).await;
    assert_eq!(state.oracles[0].1.token_exchange_rate, 500);

    // multisig troca o beneficiary do IGP via governor
    let new_beneficiary = Pubkey::new_unique();
    let ix = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(multisig.pubkey(), true),
            AccountMeta::new_readonly(env.config, false),
            AccountMeta::new_readonly(env.igp_id, false),
            AccountMeta::new(env.igp_account, false),
        ],
        data: borsh::to_vec(&GovInstruction::SetIgpBeneficiary(new_beneficiary)).unwrap(),
    };
    send(&mut env.ctx, &[ix], &[multisig]).await.unwrap();
    assert_eq!(igp_state(&mut env).await.beneficiary, new_beneficiary);
}

#[tokio::test]
async fn emergency_transfer_igp_ownership() {
    let mut env = setup().await;
    let multisig = env.multisig.insecure_clone();

    // SAÍDA DE EMERGÊNCIA: devolve a posse do IGP ao multisig
    let ix = Instruction {
        program_id: env.gov_id,
        accounts: vec![
            AccountMeta::new(multisig.pubkey(), true),
            AccountMeta::new_readonly(env.config, false),
            AccountMeta::new_readonly(env.igp_id, false),
            AccountMeta::new(env.igp_account, false),
        ],
        data: borsh::to_vec(&GovInstruction::TransferIgpOwnership(Some(multisig.pubkey())))
            .unwrap(),
    };
    send(&mut env.ctx, &[ix], &[multisig.insecure_clone()]).await.unwrap();
    assert_eq!(igp_state(&mut env).await.owner, Some(multisig.pubkey()));

    // e o governor PERDEU o poder: quórum de preço agora falha na CPI
    let (s0, s1) = (env.ops[0].insecure_clone(), env.ops[1].insecure_clone());
    let ix = submit_ix(&env, &s0, epoch_now(), 100, 10);
    send(&mut env.ctx, &[ix], &[s0]).await.unwrap();
    let ix = submit_ix(&env, &s1, epoch_now(), 100, 10);
    let err = send(&mut env.ctx, &[ix], &[s1]).await.unwrap_err();
    assert!(!err.is_empty()); // CPI rejeitada pelo IGP (owner mudou)
}
