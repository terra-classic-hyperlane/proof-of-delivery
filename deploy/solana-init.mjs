// Phase 4 init (Solana mainnet): SINGLE `pod` program (vault + governor merged
// to pay the runtime rent only once — 1.29 SOL instead of 1.9).
// The 1st byte of the instruction data routes: 0x00=rrv(vault) · 0x01=governor.
// Usage:  node deploy/solana-init.mjs <POD_PROGRAM_ID> [--vault-only] [--transfer-igp] [--set-beneficiary] [--seed]
// Keypair: SOLANA_KEYPAIR (default: keypair of the IGP's current owner).
// Requires the oracle-agent's node_modules (symlink deploy/node_modules).
import fs from "node:fs";
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const KEYPAIR = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";

// real addresses (WARP-GAS-CONFIG.md · 2026-08-18)
const IGP_PROGRAM = new PublicKey("FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR");
const IGP_INNER   = new PublicKey("FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk");

// parameters (docs/PARAMETROS_PROPOSTA.md, REAL on-chain convention)
const TC_DOMAIN = 132556;
const REWARD_LAMPORTS = 3_000_000n;      // 0.003 SOL
const EPOCH_SECS = 21_600n;
const DELTA_BPS = 2_000n;
// FALLBACK only (snapshot of 07-09: rate 2.94e10 · gas 28325) — the REAL value is
// read from the PRODUCTION Igp in readCurrentGasData(); the docs age, the chain doesn't.
const FALLBACK = { rate: 29_400_000_000n, gas: 28_325n };
const TOKEN_DECIMALS = 6;
const SEED_LAMPORTS = 300_000_000n;      // 0.3 SOL (100× fee)

const positional = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const VAULT_ONLY = process.argv.includes("--vault-only");
const podId = positional[0] ? new PublicKey(positional[0]) : null;
if (!podId) {
  console.error("usage: solana-init.mjs <POD_PROGRAM_ID> [--vault-only] [--transfer-igp] [--set-beneficiary] [--seed]");
  process.exit(1);
}
// pod = vault + governor on the same program id; module chosen by the 1st byte
const rrvId = podId, govId = podId;
const MOD_RRV = Buffer.from([0]), MOD_GOV = Buffer.from([1]);
const DO_TRANSFER = process.argv.includes("--transfer-igp");
const DO_BENEFICIARY = process.argv.includes("--set-beneficiary");
const DO_SEED = process.argv.includes("--seed");

// operators: signer + (optional) OPERATOR2 via env; quorum follows (docs/OPERADORES.md)
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
console.log("rrv config PDA (the POOL / beneficiary):", rrvConfig.toBase58());
console.log("gov config PDA (future owner of the IGP):", govConfig.toBase58());

/** Reads the CURRENT RemoteGasData of domain 132556 straight from the production Igp.
 *  Verified on-chain 2026-08-18: the account is `01`(initialized) + "IGP_____"
 *  (disc 8B) + bump + salt[32] + Option<owner> + beneficiary[32] + HashMap.
 *  Instead of fixed offsets (fragile — they broke once already), it SCANS the buffer
 *  for the domain LE and validates the [domain u32][variant 0][rate u128]
 *  [gas u128][decimals u8] entry. Fallback warned if nothing plausible is found. */
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
      // sanity: RemoteGasData variant, plausible values
      if (variant === 0 && rate > 0n && gas > 0n && decimals >= 1 && decimals <= 18) {
        return { rate, gas, decimals };
      }
    }
    throw new Error(`plausible entry for domain ${TC_DOMAIN} not found (${d.length} bytes)`);
  } catch (e) {
    console.warn(`⚠️ reading the production Igp failed (${e.message}) — using FALLBACK of 07-09. CHECK!`);
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
if (await exists(rrvConfig)) console.log("· rrv already initialized");
else await send("rrv Init", new TransactionInstruction({
  programId: rrvId,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: rrvConfig, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([MOD_RRV, u8(0), vecPk(OPS(kp.publicKey)), u8(QUORUM), u64(REWARD_LAMPORTS), u64(EPOCH_SECS)]),
}));

// ---- VAULT_ONLY: points the IGP beneficiary straight to the rrv pool and finishes.
//      (the IGP's current owner, this keypair, signs SetIgpBeneficiary=variant 9;
//       accounts [system, igp w, owner signer].) No governor.
if (VAULT_ONLY) {
  // IgpInstruction::SetIgpBeneficiary(Pubkey) = variant 7 · accounts [igp w, owner signer]
  await send("IGP SetIgpBeneficiary → rrv pool (directly by the owner)", new TransactionInstruction({
    programId: IGP_PROGRAM,
    keys: [
      { pubkey: IGP_INNER, isSigner: false, isWritable: true },
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    ],
    data: Buffer.concat([u8(7), pk(rrvConfig)]),
  }));
  if (DO_SEED) {
    await send("seed 0.3 SOL → rrv pool", SystemProgram.transfer({
      fromPubkey: kp.publicKey, toPubkey: rrvConfig, lamports: Number(SEED_LAMPORTS),
    }));
  }
  console.log("\n✓ VAULT-ONLY ready: rrv initialized + IGP.beneficiary = pool.");
  console.log("  The price/oracle stays with the IGP's current owner; the governor comes in Phase 4b.");
  process.exit(0);
}

// ---- 2. governor Init ----
if (await exists(govConfig)) console.log("· governor already initialized");
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

// ---- 3. SetDomainConfig(132556) — bounds derived from the PRODUCTION Igp now ----
if (await exists(govDomain)) console.log("· domain already configured");
else {
  const cur = await readCurrentGasData();
  const b = {
    minRate: cur.rate / 3n > 0n ? cur.rate / 3n : 1n, maxRate: cur.rate * 3n,
    minGas: cur.gas / 3n > 0n ? cur.gas / 3n : 1n, maxGas: cur.gas * 3n,
  };
  console.log(`current in the Igp: rate=${cur.rate} gas=${cur.gas} decimals=${cur.decimals} → bounds [${b.minRate}·${b.maxRate}] [${b.minGas}·${b.maxGas}]`);
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

// ---- 4. lamports for the governor's config PDA (the IGP realloc charges the owner) ----
const govBal = await conn.getBalance(govConfig);
if (govBal < 0.05 * LAMPORTS_PER_SOL) {
  await send("top-up gov config PDA (0.05 SOL)", SystemProgram.transfer({
    fromPubkey: kp.publicKey, toPubkey: govConfig, lamports: 50_000_000,
  }));
}

// ---- 5. (optional, IRREVERSIBLE without the governor) IGP ownership → governor ----
if (DO_TRANSFER) {
  // instruction of the real IGP PROGRAM: TransferIgpOwnership(Option<Pubkey>) = variant 5
  // accounts: [igp w, owner signer] — signed by the CURRENT owner (this keypair)
  await send("IGP TransferIgpOwnership → gov config PDA", new TransactionInstruction({
    programId: IGP_PROGRAM,
    keys: [
      { pubkey: IGP_INNER, isSigner: false, isWritable: true },
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    ],
    data: Buffer.concat([u8(5), u8(1), pk(govConfig)]), // Some(govConfig)
  }));
} else {
  console.log("→ IGP ownership NOT transferred (run with --transfer-igp after the devnet test)");
}

// ---- 6. (optional) IGP beneficiary → rrv pool (via governor, post-transfer) ----
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

// ---- 7. (optional) pool seed ----
if (DO_SEED) {
  await send("seed 0.3 SOL → rrv pool", SystemProgram.transfer({
    fromPubkey: kp.publicKey, toPubkey: rrvConfig, lamports: Number(SEED_LAMPORTS),
  }));
}

console.log("\ndone. SECURITY TODOs (§8 of the handoff):");
console.log("  solana program set-upgrade-authority <POD_ID> --new-upgrade-authority <MULTISIG>");
