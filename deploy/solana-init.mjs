// Init da Fase 4 (Solana mainnet): programa ÚNICO `pod` (vault + governor fundidos
// p/ pagar o rent da runtime uma vez só — 1,29 SOL em vez de 1,9).
// O 1º byte do instruction data roteia: 0x00=rrv(vault) · 0x01=governor.
// Uso:  node deploy/solana-init.mjs <POD_PROGRAM_ID> [--vault-only] [--transfer-igp] [--set-beneficiary] [--seed]
// Keypair: SOLANA_KEYPAIR (default: keypair do owner atual do IGP).
// Requer os node_modules do oracle-agent (symlink deploy/node_modules).
import fs from "node:fs";
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const KEYPAIR = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";

// endereços reais (WARP-GAS-CONFIG.md · 18/08/2026)
const IGP_PROGRAM = new PublicKey("FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR");
const IGP_INNER   = new PublicKey("FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk");

// parâmetros (docs/PARAMETROS_PROPOSTA.md, convenção REAL da chain)
const TC_DOMAIN = 132556;
const REWARD_LAMPORTS = 3_000_000n;      // 0,003 SOL
const EPOCH_SECS = 21_600n;
const DELTA_BPS = 2_000n;
// FALLBACK apenas (retrato de 09/07: rate 2,94e10 · gas 28325) — o valor REAL é
// lido do Igp de PRODUÇÃO em readCurrentGasData(); a doc envelhece, a chain não.
const FALLBACK = { rate: 29_400_000_000n, gas: 28_325n };
const TOKEN_DECIMALS = 6;
const SEED_LAMPORTS = 300_000_000n;      // 0,3 SOL (100× tarifa)

const positional = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const VAULT_ONLY = process.argv.includes("--vault-only");
const podId = positional[0] ? new PublicKey(positional[0]) : null;
if (!podId) {
  console.error("uso: solana-init.mjs <POD_PROGRAM_ID> [--vault-only] [--transfer-igp] [--set-beneficiary] [--seed]");
  process.exit(1);
}
// pod = vault + governor no mesmo program id; módulo escolhido pelo 1º byte
const rrvId = podId, govId = podId;
const MOD_RRV = Buffer.from([0]), MOD_GOV = Buffer.from([1]);
const DO_TRANSFER = process.argv.includes("--transfer-igp");
const DO_BENEFICIARY = process.argv.includes("--set-beneficiary");
const DO_SEED = process.argv.includes("--seed");

// operadores: signer + (opcional) OPERATOR2 via env; quórum acompanha (docs/OPERADORES.md)
const OPERATOR2 = process.env.OPERATOR2 ? new PublicKey(process.env.OPERATOR2) : null;
const OPS = (me) => (OPERATOR2 ? [me, OPERATOR2] : [me]);
const QUORUM = OPERATOR2 ? 2 : 1;

const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(KEYPAIR, "utf8"))));
const conn = new Connection(RPC, "confirmed");
console.log("signer:", kp.publicKey.toBase58());

// ---- borsh helpers ----
const u8 = (n) => Buffer.from([Number(n)]);
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const u128 = (n) => { const b = Buffer.alloc(16); let v = BigInt(n); for (let i = 0; i < 16; i++) { b[i] = Number(v & 0xffn); v >>= 8n; } return b; };
const pk = (p) => Buffer.from(p.toBytes());
const vecPk = (arr) => Buffer.concat([u32(arr.length), ...arr.map(pk)]);
const sep = Buffer.from("-");

const pda = (programId, seeds) => PublicKey.findProgramAddressSync(seeds, programId)[0];
const rrvConfig = pda(rrvId, [Buffer.from("rrv"), sep, Buffer.from("config")]);
const govConfig = pda(govId, [Buffer.from("gov"), sep, Buffer.from("config")]);
const govDomain = pda(govId, [Buffer.from("gov"), sep, Buffer.from("domain"), sep, u32(TC_DOMAIN)]);
console.log("rrv config PDA (o POOL / beneficiary):", rrvConfig.toBase58());
console.log("gov config PDA (futuro owner do IGP):", govConfig.toBase58());

/** Lê o RemoteGasData VIGENTE do domínio 132556 direto do Igp de produção.
 *  Verificado on-chain 18/08/2026: o account é `01`(initialized) + "IGP_____"
 *  (disc 8B) + bump + salt[32] + Option<owner> + beneficiary[32] + HashMap.
 *  Em vez de offsets fixos (frágeis — já quebraram uma vez), VARRE o buffer
 *  pelo domínio LE e valida a entrada [domain u32][variante 0][rate u128]
 *  [gas u128][decimals u8]. Fallback avisado se nada plausível for achado. */
async function readCurrentGasData() {
  try {
    const info = await conn.getAccountInfo(IGP_INNER);
    const d = info.data;
    const needle = u32(TC_DOMAIN);
    const readU128 = (p) => { let v = 0n; for (let i = 15; i >= 0; i--) v = (v << 8n) | BigInt(d[p + i]); return v; };
    let idx = -1;
    while ((idx = d.indexOf(needle, idx + 1)) !== -1) {
      if (idx + 38 > d.length) break;
      const variant = d[idx + 4];
      const rate = readU128(idx + 5);
      const gas = readU128(idx + 21);
      const decimals = d[idx + 37];
      // sanidade: variante RemoteGasData, valores plausíveis
      if (variant === 0 && rate > 0n && gas > 0n && decimals >= 1 && decimals <= 18) {
        return { rate, gas, decimals };
      }
    }
    throw new Error(`entrada plausível do domínio ${TC_DOMAIN} não encontrada (${d.length} bytes)`);
  } catch (e) {
    console.warn(`⚠️ leitura do Igp de produção falhou (${e.message}) — usando FALLBACK de 09/07. CONFIRA!`);
    return { rate: FALLBACK.rate, gas: FALLBACK.gas, decimals: TOKEN_DECIMALS };
  }
}

async function send(label, ix) {
  const tx = new Transaction().add(ix);
  const sig = await conn.sendTransaction(tx, [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  console.log(`✓ ${label}: ${sig}`);
}
const exists = async (p) => !!(await conn.getAccountInfo(p));

// ---- 1. rrv Init ----
if (await exists(rrvConfig)) console.log("· rrv já inicializado");
else await send("rrv Init", new TransactionInstruction({
  programId: rrvId,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: rrvConfig, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([MOD_RRV, u8(0), vecPk(OPS(kp.publicKey)), u8(QUORUM), u64(REWARD_LAMPORTS), u64(EPOCH_SECS)]),
}));

// ---- VAULT_ONLY: aponta o beneficiary do IGP direto p/ o pool do rrv e encerra.
//      (o owner atual do IGP, este keypair, assina SetIgpBeneficiary=variante 9;
//       contas [system, igp w, owner signer].) Sem governor.
if (VAULT_ONLY) {
  // IgpInstruction::SetIgpBeneficiary(Pubkey) = variante 7 · contas [igp w, owner signer]
  await send("IGP SetIgpBeneficiary → rrv pool (direto pelo owner)", new TransactionInstruction({
    programId: IGP_PROGRAM,
    keys: [
      { pubkey: IGP_INNER, isSigner: false, isWritable: true },
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    ],
    data: Buffer.concat([u8(7), pk(rrvConfig)]),
  }));
  if (DO_SEED) {
    await send("seed 0,3 SOL → rrv pool", SystemProgram.transfer({
      fromPubkey: kp.publicKey, toPubkey: rrvConfig, lamports: Number(SEED_LAMPORTS),
    }));
  }
  console.log("\n✓ VAULT-ONLY pronto: rrv inicializado + IGP.beneficiary = pool.");
  console.log("  O preço/oracle segue com o owner atual do IGP; o governor entra na Fase 4b.");
  process.exit(0);
}

// ---- 2. governor Init ----
if (await exists(govConfig)) console.log("· governor já inicializado");
else await send("governor Init", new TransactionInstruction({
  programId: govId,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: govConfig, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([
    MOD_GOV, u8(0), pk(kp.publicKey), vecPk(OPS(kp.publicKey)), u8(QUORUM),
    u64(EPOCH_SECS), u64(DELTA_BPS), pk(IGP_PROGRAM), pk(IGP_INNER),
  ]),
}));

// ---- 3. SetDomainConfig(132556) — faixas derivadas do Igp de PRODUÇÃO agora ----
if (await exists(govDomain)) console.log("· domínio já configurado");
else {
  const cur = await readCurrentGasData();
  const b = {
    minRate: cur.rate / 3n > 0n ? cur.rate / 3n : 1n, maxRate: cur.rate * 3n,
    minGas: cur.gas / 3n > 0n ? cur.gas / 3n : 1n, maxGas: cur.gas * 3n,
  };
  console.log(`vigente no Igp: rate=${cur.rate} gas=${cur.gas} decimals=${cur.decimals} → faixas [${b.minRate}·${b.maxRate}] [${b.minGas}·${b.maxGas}]`);
  await send("governor SetDomainConfig(132556)", new TransactionInstruction({
    programId: govId,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: govConfig, isSigner: false, isWritable: false },
      { pubkey: govDomain, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([
      MOD_GOV, u8(2), u32(TC_DOMAIN),
      u128(b.minRate), u128(b.maxRate), u128(b.minGas), u128(b.maxGas),
      u8(cur.decimals),
    ]),
  }));
}

// ---- 4. lamports p/ a config PDA do governor (realloc do IGP cobra do owner) ----
const govBal = await conn.getBalance(govConfig);
if (govBal < 0.05 * LAMPORTS_PER_SOL) {
  await send("top-up gov config PDA (0,05 SOL)", SystemProgram.transfer({
    fromPubkey: kp.publicKey, toPubkey: govConfig, lamports: 50_000_000,
  }));
}

// ---- 5. (opcional, IRREVERSÍVEL sem o governor) posse do IGP → governor ----
if (DO_TRANSFER) {
  // instrução do PROGRAMA IGP real: TransferIgpOwnership(Option<Pubkey>) = variante 5
  // contas: [igp w, owner signer] — assinada pelo owner ATUAL (este keypair)
  await send("IGP TransferIgpOwnership → gov config PDA", new TransactionInstruction({
    programId: IGP_PROGRAM,
    keys: [
      { pubkey: IGP_INNER, isSigner: false, isWritable: true },
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    ],
    data: Buffer.concat([u8(5), u8(1), pk(govConfig)]), // Some(govConfig)
  }));
} else {
  console.log("→ posse do IGP NÃO transferida (rode com --transfer-igp após o teste em devnet)");
}

// ---- 6. (opcional) beneficiary do IGP → pool do rrv (via governor, pós-transfer) ----
if (DO_BENEFICIARY) {
  await send("governor SetIgpBeneficiary → rrv pool", new TransactionInstruction({
    programId: govId,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
      { pubkey: govConfig, isSigner: false, isWritable: true },
      { pubkey: IGP_PROGRAM, isSigner: false, isWritable: false },
      { pubkey: IGP_INNER, isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([MOD_GOV, u8(9), pk(rrvConfig)]),
  }));
}

// ---- 7. (opcional) semente do pool ----
if (DO_SEED) {
  await send("seed 0,3 SOL → rrv pool", SystemProgram.transfer({
    fromPubkey: kp.publicKey, toPubkey: rrvConfig, lamports: Number(SEED_LAMPORTS),
  }));
}

console.log("\nfeito. PENDÊNCIAS DE SEGURANÇA (§8 do handoff):");
console.log("  solana program set-upgrade-authority <POD_ID> --new-upgrade-authority <MULTISIG>");
