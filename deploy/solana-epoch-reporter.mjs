// solana-epoch-reporter — reporter do QUÓRUM p/ o sentido TC→Solana (sem keeper).
//
// O relayer NATIVO entrega as msgs TC→Solana na Solana (nada muda). Este reporter
// OBSERVA off-chain quem entregou (fee payer da tx de entrega) e submete o
// `EpochReport` ao `pod`. Quando um QUÓRUM de operadores submete o MESMO relatório
// (hash idêntico), o contrato credita cada operador; cada um saca do pool.
//
// Determinístico p/ o quórum: cada entrega é atribuída a uma época pelo blockTime do
// seu slot (tudo lido da chain), então todos os operadores chegam ao MESMO relatório.
//
//   uso:
//     node deploy/solana-epoch-reporter.mjs               # DRY: mostra o relatório
//     node deploy/solana-epoch-reporter.mjs --epoch <E>   # DRY de uma época específica
//     node deploy/solana-epoch-reporter.mjs --submit      # submete a última época fechada
//     node deploy/solana-epoch-reporter.mjs --submit --epoch <E>
//   chave: SOLANA_KEYPAIR (default BirXd4Q…) — precisa ser um operador do rrv.
import fs from "node:fs";
import {
  Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction,
} from "@solana/web3.js";
import { keccak_256 } from "@noble/hashes/sha3";
import bs58 from "bs58";

// carrega a chave do operador de SOLANA_PRIVATE_KEY (base58 / hex / JSON array) —
// mesmo formato do relayer — ou do arquivo SOLANA_KEYPAIR (fallback).
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
const POD_32 = "1a3be2685e7a787a1bedadcc90889b367f8fe72240de5aa43e4c2b88d07776a2"; // exclui recibos
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

// ---- 1. varre os dispatches TC→Solana (transferências, exclui recibos ao pod) ----
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
    if (recipientOf(msg) === POD_32) continue; // é recibo, não entrega de token
    out.push(Buffer.from(msg, "hex"));
  }
  return out;
}

// ---- 2. p/ cada msg: entregue na Solana? em que slot? quem entregou (fee payer)? ----
async function resolveDelivery(message) {
  const id = Buffer.from(keccak_256(message));
  const pm = processedPda(id);
  const info = await conn.getAccountInfo(pm);
  if (!info) return null; // ainda não entregue
  // AccountData: [init u8=01][disc "PROCESSD" 8][sequence u64][message_id 32][slot u64]
  const slot = Number(info.data.readBigUInt64LE(1 + 8 + 8 + 32)); // offset 49
  const sigs = await conn.getSignaturesForAddress(pm, { limit: 1000 });
  if (!sigs.length) return null;
  const creation = sigs[sigs.length - 1]; // a mais antiga = a criação = a entrega
  const tx = await conn.getTransaction(creation.signature, { maxSupportedTransactionVersion: 0 });
  if (!tx) return null; // tx podada no RPC público (precisa de archive p/ entregas antigas)
  const m = tx.transaction.message;
  const payer = (m.staticAccountKeys ?? m.accountKeys)[0]; // fee payer = conta 0 (sempre estática)
  return { id: "0x" + id.toString("hex"), slot, payer, blockTime: tx.blockTime };
}

async function main() {
  const msgs = await scanTcSolanaMessages();
  console.log(`dispatches TC→Solana (transferências): ${msgs.length}`);
  const dels = [];
  for (const m of msgs) {
    const d = await resolveDelivery(m).catch(() => null);
    if (d && d.blockTime) dels.push(d);
  }
  console.log(`entregues na Solana: ${dels.length}`);
  // atribui cada entrega a uma época pelo blockTime (determinístico)
  for (const d of dels) d.epoch = Math.floor(d.blockTime / EPOCH_SECS);
  const nowEpoch = Math.floor(Date.now() / 1000 / EPOCH_SECS);
  const target = FORCE_EPOCH ?? Math.max(...dels.map((d) => d.epoch).filter((e) => e < nowEpoch), -1);
  if (target < 0) { console.log("nenhuma época fechada com entregas."); return; }
  const inEpoch = dels.filter((d) => d.epoch === target);
  if (!inEpoch.length) { console.log(`época ${target}: sem entregas.`); return; }

  // operadores registrados (config) — por padrão só credita esses (não paga estranhos
  // do pool). INCLUDE_ALL=1 credita QUALQUER um que entregou (modo permissionless).
  const cfg = (await conn.getAccountInfo(CONFIG)).data;
  const nOps = cfg.readUInt32LE(1 + 1 + 8 + 8 + 1);
  const registered = new Set();
  for (let i = 0, o = 1 + 1 + 8 + 8 + 1 + 4; i < nOps; i++, o += 32) registered.add(new PublicKey(cfg.subarray(o, o + 32)).toBase58());
  const includeAll = process.env.INCLUDE_ALL === "1";

  // agrega por operador (fee payer) e ORDENA por pubkey (regra de convergência §09)
  const byOp = new Map();
  for (const d of inEpoch) {
    const k = d.payer.toBase58();
    if (!includeAll && !registered.has(k)) continue; // pula relayer não-registrado
    byOp.set(k, (byOp.get(k) ?? 0) + 1);
  }
  if (!byOp.size) { console.log(`época ${target}: nenhuma entrega de operador registrado (use INCLUDE_ALL=1).`); return; }
  const credits = [...byOp.entries()]
    .map(([k, n]) => ({ op: new PublicKey(k), count: n }))
    .sort((a, b) => Buffer.compare(a.op.toBuffer(), b.op.toBuffer()));
  const slots = inEpoch.map((d) => d.slot);
  const windowStart = Math.min(...slots), windowEnd = Math.max(...slots);

  console.log(`\n=== EpochReport (época ${target}, época atual ${nowEpoch}) ===`);
  console.log(`window slots: ${windowStart}..${windowEnd}`);
  for (const c of credits) console.log(`  ${c.op.toBase58()} : ${c.count} entrega(s)`);

  // borsh do EpochReport: epoch u64, ws u64, we u64, credits Vec<(Pubkey,u64)>, remote Vec<...>
  const creditsBuf = Buffer.concat([
    u32(credits.length),
    ...credits.flatMap((c) => [Buffer.from(c.op.toBytes()), u64(c.count)]),
  ]);
  const report = Buffer.concat([u64(target), u64(windowStart), u64(windowEnd), creditsBuf, u32(0)]);
  const data = Buffer.concat([Buffer.from([0, 1]), report]); // [módulo rrv][SubmitEpochReport]

  const epochPda = pda([Buffer.from("rrv"), sep, Buffer.from("epoch"), sep, u64(target)]);
  const creditPdas = credits.map((c) => pda([Buffer.from("rrv"), sep, Buffer.from("credit"), sep, Buffer.from(c.op.toBytes())]));

  if (!SUBMIT) { console.log("\n[DRY] use --submit p/ enviar (assina como operador do rrv)."); return; }

  const kp = loadSolanaKp();
  console.log("signatário:", kp.publicKey.toBase58(),
    registered.has(kp.publicKey.toBase58()) ? "(operador ✓)" : "(⚠ NÃO é operador registrado — submit falhará)");
  const keys = [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    // CONFIG É ESCRITO pelo programa (total_credited + bitmap de replay das
    // épocas) → precisa ser writable. (era read-only por engano; nunca aparecia
    // porque o submit falhava antes por falta de SOL.)
    { pubkey: CONFIG, isSigner: false, isWritable: true },
    { pubkey: epochPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ...creditPdas.map((p) => ({ pubkey: p, isSigner: false, isWritable: true })),
  ];
  const ix = new TransactionInstruction({ programId: POD, keys, data });
  try {
    const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    console.log("✓ EpochReport submetido:", sig);
  } catch (e) {
    const m = String(e.message ?? e);
    // 0x66=janela travada, 0x68=época ainda aberta, 0x69=já aplicada → nada a fazer.
    // Outros (0x64 not_operator, 0x65 paused, 0x67 unsorted) são erros reais.
    if (/custom program error: 0x6[689]\b/.test(m) || m.includes("0x69") || m.includes("0x68") || m.includes("0x66")) console.log("· época já reportada/aberta — nada a fazer");
    else throw e;
  }
}

// saca automaticamente o crédito acumulado do operador (Withdraw do pod → SOL na carteira)
async function withdrawOwn() {
  if (!SUBMIT) return;
  let kp; try { kp = loadSolanaKp(); } catch { return; }
  const [creditPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("rrv"), sep, Buffer.from("credit"), sep, kp.publicKey.toBuffer()], POD);
  const info = await conn.getAccountInfo(creditPda);
  if (!info) return;
  const avail = info.data.readBigUInt64LE(1 + 32) - info.data.readBigUInt64LE(1 + 32 + 8); // credited - withdrawn
  if (avail <= 0n) return;
  const data = Buffer.concat([Buffer.from([0, 2]), u64(avail)]); // Withdraw{amount}=variante 2
  const keys = [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: CONFIG, isSigner: false, isWritable: true },
    { pubkey: creditPda, isSigner: false, isWritable: true },
  ];
  try {
    const sig = await conn.sendTransaction(new Transaction().add(new TransactionInstruction({ programId: POD, keys, data })), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    console.log(`✓ saque ${avail} lamports p/ ${kp.publicKey.toBase58()}:`, sig);
  } catch (e) { console.log("saque erro:", String(e.message ?? e).slice(0, 120)); }
}

async function tick() {
  await main().catch((e) => console.error("ERRO:", e.message));
  await withdrawOwn().catch((e) => console.error("saque ERRO:", e.message));
}
if (LOOP_SECS > 0) {
  console.log(`reporter+saque em loop a cada ${LOOP_SECS}s`);
  for (;;) { await tick(); await new Promise((r) => setTimeout(r, LOOP_SECS * 1000)); }
} else {
  await tick();
}
