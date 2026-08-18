// Init da Fase 4 (Solana mainnet): rrv + igp-oracle-governor.
// Uso:  node deploy/solana-init.mjs <RRV_PROGRAM_ID> <GOV_PROGRAM_ID> [--transfer-igp] [--set-beneficiary] [--seed]
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
// oracle vigente: rate 2,94e10 · gas 28325 (recalibrado 09/07) → faixa ÷3/×3
const BOUNDS = { minRate: 9_800_000_000n, maxRate: 88_200_000_000n, minGas: 9_442n, maxGas: 84_975n };
const TOKEN_DECIMALS = 6;
const SEED_LAMPORTS = 300_000_000n;      // 0,3 SOL (100× tarifa)

const [rrvId, govId] = process.argv.slice(2).filter((a) => !a.startsWith("--")).map((a) => new PublicKey(a));
if (!rrvId || !govId) { console.error("uso: solana-init.mjs <RRV_ID> <GOV_ID> [--transfer-igp] [--set-beneficiary] [--seed]"); process.exit(1); }
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
  data: Buffer.concat([u8(0), vecPk(OPS(kp.publicKey)), u8(QUORUM), u64(REWARD_LAMPORTS), u64(EPOCH_SECS)]),
}));

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
    u8(0), pk(kp.publicKey), vecPk(OPS(kp.publicKey)), u8(QUORUM),
    u64(EPOCH_SECS), u64(DELTA_BPS), pk(IGP_PROGRAM), pk(IGP_INNER),
  ]),
}));

// ---- 3. SetDomainConfig(132556) ----
if (await exists(govDomain)) console.log("· domínio já configurado");
else await send("governor SetDomainConfig(132556)", new TransactionInstruction({
  programId: govId,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: govConfig, isSigner: false, isWritable: false },
    { pubkey: govDomain, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([
    u8(2), u32(TC_DOMAIN),
    u128(BOUNDS.minRate), u128(BOUNDS.maxRate), u128(BOUNDS.minGas), u128(BOUNDS.maxGas),
    u8(TOKEN_DECIMALS),
  ]),
}));

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
    data: Buffer.concat([u8(9), pk(rrvConfig)]),
  }));
}

// ---- 7. (opcional) semente do pool ----
if (DO_SEED) {
  await send("seed 0,3 SOL → rrv pool", SystemProgram.transfer({
    fromPubkey: kp.publicKey, toPubkey: rrvConfig, lamports: Number(SEED_LAMPORTS),
  }));
}

console.log("\nfeito. PENDÊNCIAS DE SEGURANÇA (§8 do handoff):");
console.log("  solana program set-upgrade-authority <RRV_ID e GOV_ID> --new-upgrade-authority <MULTISIG>");
