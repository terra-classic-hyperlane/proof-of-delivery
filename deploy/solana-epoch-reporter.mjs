// solana-epoch-reporter — QUORUM reporter for the TC→Solana direction (no keeper).
//
// The NATIVE relayer delivers the TC→Solana msgs on Solana (nothing changes). This
// reporter OBSERVES off-chain who delivered (fee payer of the delivery tx) and submits
// the `EpochReport` to the `pod`. When a QUORUM of operators submits the SAME report
// (identical hash), the contract credits each operator; each one withdraws from the pool.
//
// Deterministic for the quorum: each delivery is assigned to an epoch by the blockTime of
// its slot (all read from the chain), so all operators arrive at the SAME report.
//
//   usage:
//     node deploy/solana-epoch-reporter.mjs               # DRY: shows the report
//     node deploy/solana-epoch-reporter.mjs --epoch <E>   # DRY of a specific epoch
//     node deploy/solana-epoch-reporter.mjs --submit      # submits the last closed epoch
//     node deploy/solana-epoch-reporter.mjs --submit --epoch <E>
//   key: SOLANA_KEYPAIR (default BirXd4Q…) — must be an rrv operator.
import fs from "node:fs";
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction,
} from "@solana/web3.js";
import { keccak_256 } from "@noble/hashes/sha3";
import bs58 from "bs58";

// loads the operator key from SOLANA_PRIVATE_KEY (base58 / hex / JSON array) —
// same format as the relayer — or from the SOLANA_KEYPAIR file (fallback).
function loadSolanaKp() {
  const env = process.env.SOLANA_PRIVATE_KEY;
  if (env && env.trim()) {
    const s = env.trim();
    let b;
    if (s.startsWith("[")) b = Uint8Array.from(JSON.parse(s));
    else if (/^(0x)?[0-9a-fA-F]+$/.test(s) && [64, 128].includes(s.replace(/^0x/, "").length)) b = Uint8Array.from(Buffer.from(s.replace(/^0x/, ""), "hex"));
    else b = bs58.decode(s);
    return b.length === 64 ? Keypair.fromSecretKey(b) : Keypair.fromSeed(b);
  }
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
    process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
}

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");
const MAILBOX = new PublicKey("E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi");
const POD_32 = "1a3be2685e7a787a1bedadcc90889b367f8fe72240de5aa43e4c2b88d07776a2"; // excludes receipts
const TC_RPC = process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io";
const TC_MAILBOX = "terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9";
const EPOCH_SECS = 21600; // = config.epoch_duration_secs

const SUBMIT = process.argv.includes("--submit");
const epIdx = process.argv.indexOf("--epoch");
const FORCE_EPOCH = epIdx > -1 ? Number(process.argv[epIdx + 1]) : null;
const loopIdx = process.argv.indexOf("--loop");
const LOOP_SECS = loopIdx > -1 ? Number(process.argv[loopIdx + 1] || 3600) : 0;

const conn = new Connection(RPC, "confirmed");
const sep = Buffer.from("-");
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];
const recipientOf = (hex) => Buffer.from(hex, "hex").subarray(45, 77).toString("hex");
const processedPda = (id) => PublicKey.findProgramAddressSync(
  [Buffer.from("hyperlane"), sep, Buffer.from("processed_message"), sep, id], MAILBOX)[0];

// ---- 1. scan the TC→Solana dispatches (transfers, excludes receipts to the pod) ----
async function scanTcSolanaMessages() {
  const q = encodeURIComponent(`"wasm-mailbox_dispatch._contract_address='${TC_MAILBOX}'"`);
  const r = await fetch(`${TC_RPC}/tx_search?query=${q}&per_page=100&order_by="desc"`).then((x) => x.json());
  const out = [];
  for (const t of r.result?.txs ?? []) {
    let dest = null, msg = null;
    for (const e of t.tx_result?.events ?? []) {
      const a = Object.fromEntries(e.attributes.map((x) => [x.key, x.value]));
      if (e.type === "wasm-mailbox_dispatch") { dest = a.destination; msg = a.message; }
    }
    if (dest !== "1399811149" || !msg) continue;
    if (recipientOf(msg) === POD_32) continue; // it's a receipt, not a token delivery
    out.push(Buffer.from(msg, "hex"));
  }
  return out;
}

// ---- 2. for each msg: delivered on Solana? in which slot? who delivered (fee payer)? ----
async function resolveDelivery(message) {
  const id = Buffer.from(keccak_256(message));
  const pm = processedPda(id);
  const info = await conn.getAccountInfo(pm);
  if (!info) return null; // not yet delivered
  // AccountData: [init u8=01][disc "PROCESSD" 8][sequence u64][message_id 32][slot u64]
  const slot = Number(info.data.readBigUInt64LE(1 + 8 + 8 + 32)); // offset 49
  const sigs = await conn.getSignaturesForAddress(pm, { limit: 1000 });
  if (!sigs.length) return null;
  const creation = sigs[sigs.length - 1]; // the oldest = the creation = the delivery
  const tx = await conn.getTransaction(creation.signature, { maxSupportedTransactionVersion: 0 });
  if (!tx) return null; // tx pruned on the public RPC (needs archive for old deliveries)
  const m = tx.transaction.message;
  const payer = (m.staticAccountKeys ?? m.accountKeys)[0]; // fee payer = account 0 (always static)
  return { id: "0x" + id.toString("hex"), slot, payer, blockTime: tx.blockTime };
}

async function main() {
  const msgs = await scanTcSolanaMessages();
  console.log(`TC→Solana dispatches (transfers): ${msgs.length}`);
  const dels = [];
  for (const m of msgs) {
    const d = await resolveDelivery(m).catch(() => null);
    if (d && d.blockTime) dels.push(d);
  }
  console.log(`delivered on Solana: ${dels.length}`);
  // assigns each delivery to an epoch by blockTime (deterministic)
  for (const d of dels) d.epoch = Math.floor(d.blockTime / EPOCH_SECS);
  const nowEpoch = Math.floor(Date.now() / 1000 / EPOCH_SECS);
  const target = FORCE_EPOCH ?? Math.max(...dels.map((d) => d.epoch).filter((e) => e < nowEpoch), -1);
  if (target < 0) { console.log("no closed epoch with deliveries."); return; }
  const inEpoch = dels.filter((d) => d.epoch === target);
  if (!inEpoch.length) { console.log(`epoch ${target}: no deliveries.`); return; }

  // registered operators (config) — by default only credits these (does not pay strangers
  // from the pool). INCLUDE_ALL=1 credits ANYONE who delivered (permissionless mode).
  const cfg = (await conn.getAccountInfo(CONFIG)).data;
  const nOps = cfg.readUInt32LE(1 + 1 + 8 + 8 + 1);
  const registered = new Set();
  for (let i = 0, o = 1 + 1 + 8 + 8 + 1 + 4; i < nOps; i++, o += 32) registered.add(new PublicKey(cfg.subarray(o, o + 32)).toBase58());
  const includeAll = process.env.INCLUDE_ALL === "1";

  // aggregate by operator (fee payer) and SORT by pubkey (convergence rule §09)
  const byOp = new Map();
  for (const d of inEpoch) {
    const k = d.payer.toBase58();
    if (!includeAll && !registered.has(k)) continue; // skip unregistered relayer
    byOp.set(k, (byOp.get(k) ?? 0) + 1);
  }
  if (!byOp.size) { console.log(`epoch ${target}: no delivery from a registered operator (use INCLUDE_ALL=1).`); return; }
  const credits = [...byOp.entries()]
    .map(([k, n]) => ({ op: new PublicKey(k), count: n }))
    .sort((a, b) => Buffer.compare(a.op.toBuffer(), b.op.toBuffer()));
  const slots = inEpoch.map((d) => d.slot);
  const windowStart = Math.min(...slots), windowEnd = Math.max(...slots);

  console.log(`\n=== EpochReport (epoch ${target}, current epoch ${nowEpoch}) ===`);
  console.log(`window slots: ${windowStart}..${windowEnd}`);
  for (const c of credits) console.log(`  ${c.op.toBase58()} : ${c.count} delivery(ies)`);

  // borsh of the EpochReport: epoch u64, ws u64, we u64, credits Vec<(Pubkey,u64)>, remote Vec<...>
  const creditsBuf = Buffer.concat([
    u32(credits.length),
    ...credits.flatMap((c) => [Buffer.from(c.op.toBytes()), u64(c.count)]),
  ]);
  const report = Buffer.concat([u64(target), u64(windowStart), u64(windowEnd), creditsBuf, u32(0)]);
  const data = Buffer.concat([Buffer.from([0, 1]), report]); // [rrv module][SubmitEpochReport]

  const epochPda = pda([Buffer.from("rrv"), sep, Buffer.from("epoch"), sep, u64(target)]);
  const creditPdas = credits.map((c) => pda([Buffer.from("rrv"), sep, Buffer.from("credit"), sep, Buffer.from(c.op.toBytes())]));

  if (!SUBMIT) { console.log("\n[DRY] use --submit to send (signs as rrv operator)."); return; }

  const kp = loadSolanaKp();
  console.log("signer:", kp.publicKey.toBase58(),
    registered.has(kp.publicKey.toBase58()) ? "(operator ✓)" : "(⚠ NOT a registered operator — submit will fail)");
  const keys = [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    // CONFIG IS WRITTEN by the program (total_credited + replay bitmap of the
    // epochs) → must be writable. (was read-only by mistake; never showed up
    // because the submit failed earlier for lack of SOL.)
    { pubkey: CONFIG, isSigner: false, isWritable: true },
    { pubkey: epochPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ...creditPdas.map((p) => ({ pubkey: p, isSigner: false, isWritable: true })),
  ];
  const ix = new TransactionInstruction({ programId: POD, keys, data });
  try {
    const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    console.log("✓ EpochReport submitted:", sig);
  } catch (e) {
    const m = String(e.message ?? e);
    // 0x66=locked window, 0x68=epoch still open, 0x69=already applied → nothing to do.
    // Others (0x64 not_operator, 0x65 paused, 0x67 unsorted) are real errors.
    if (/custom program error: 0x6[689]\b/.test(m) || m.includes("0x69") || m.includes("0x68") || m.includes("0x66")) console.log("· epoch already reported/open — nothing to do");
    else throw e;
  }
}

// automatically withdraws the operator's accumulated credit (Withdraw from pod → SOL in the wallet)
async function withdrawOwn() {
  if (!SUBMIT) return;
  let kp; try { kp = loadSolanaKp(); } catch { return; }
  const [creditPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("rrv"), sep, Buffer.from("credit"), sep, kp.publicKey.toBuffer()], POD);
  const info = await conn.getAccountInfo(creditPda);
  if (!info) return;
  const avail = info.data.readBigUInt64LE(1 + 32) - info.data.readBigUInt64LE(1 + 32 + 8); // credited - withdrawn
  if (avail <= 0n) return;
  const data = Buffer.concat([Buffer.from([0, 2]), u64(avail)]); // Withdraw{amount}=variant 2
  const keys = [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: CONFIG, isSigner: false, isWritable: true },
    { pubkey: creditPda, isSigner: false, isWritable: true },
  ];
  try {
    const sig = await conn.sendTransaction(new Transaction().add(new TransactionInstruction({ programId: POD, keys, data })), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    console.log(`✓ withdrawal ${avail} lamports to ${kp.publicKey.toBase58()}:`, sig);
  } catch (e) { console.log("withdrawal error:", String(e.message ?? e).slice(0, 120)); }
}

async function tick() {
  await main().catch((e) => console.error("ERROR:", e.message));
  await withdrawOwn().catch((e) => console.error("withdrawal ERROR:", e.message));
}
if (LOOP_SECS > 0) {
  console.log(`reporter+withdrawal in loop every ${LOOP_SECS}s`);
  for (;;) { await tick(); await new Promise((r) => setTimeout(r, LOOP_SECS * 1000)); }
} else {
  await tick();
}
