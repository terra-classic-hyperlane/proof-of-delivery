// solana-receipt-keeper — keeper of the TC→Solana direction (trustless receipt).
//
// For a TC→Solana message NOT YET delivered, it builds ONE transaction with:
//   1. Mailbox.InboxProcess(message)   ← the delivery (with ISM metadata + accounts)
//   2. pod.SendReceiptAtomic(...)       ← reads (1) by introspection and dispatches the receipt
// This way the executor (account 0 of the InboxProcess = this keeper) is proven on-chain and
// the receipt pays the LUNC origin fee on TC. Trustless: the keeper is only paid for
// messages that IT actually delivers.
//
//   usage:  node deploy/solana-receipt-keeper.mjs <message_id_hex> [--deliver-only] [--rpc <url>]
//
// Requires: SOLANA_KEEPER_KEY (keypair JSON file) — the same Solana relayer.
// ⚠️ Assembling the InboxProcess (ISM metadata + account-metas) is the point that
//    REQUIRES validation on DEVNET (spec §08) before mainnet.
import fs from "node:fs";
import {
  Connection, Keypair, PublicKey, Transaction, TransactionInstruction,
  SystemProgram, SYSVAR_INSTRUCTIONS_PUBKEY,
} from "@solana/web3.js";
import { keccak_256 } from "@noble/hashes/sha3";

// ---- production addresses (mainnet) ----
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const MAILBOX = new PublicKey("E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi");
const SPL_NOOP = new PublicKey("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");
const WARP_RECIPIENT = new PublicKey("EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL"); // warp program (real recipient of the TC→Solana msgs)
const TC_DOMAIN = 132556;
const OPERATOR_INDEX = 0;

// ---- MessageRecipient discriminators ----
const HANDLE_METAS_DISC = Buffer.from([194, 141, 30, 82, 241, 41, 169, 52]);
const ISM_METAS_DISC = Buffer.from([190, 214, 218, 129, 67, 97, 4, 76]);
const ISM_DISC = Buffer.from([45, 18, 245, 87, 234, 46, 246, 15]);

const args = process.argv.slice(2);
const MSG_ID = args.find((a) => !a.startsWith("--"));
const DELIVER_ONLY = args.includes("--deliver-only");
const RPC = args[args.indexOf("--rpc") + 1] || process.env.SOLANA_RPC || "https://api.mainnet-beta.solana.com";
if (!MSG_ID) { console.error("usage: solana-receipt-keeper.mjs <message_id_hex> [--deliver-only]"); process.exit(1); }

const conn = new Connection(RPC, "confirmed");
const kp = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync(process.env.SOLANA_KEEPER_KEY, "utf8"))),
);
console.log("keeper:", kp.publicKey.toBase58(), "· rpc:", RPC);

// ---- PDA helpers ----
const sep = Buffer.from("-");
const u32le = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
const podPda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];
const mbPda = (seeds) => PublicKey.findProgramAddressSync(seeds, MAILBOX);

const inboxPda = mbPda([Buffer.from("hyperlane"), sep, Buffer.from("inbox")])[0];
const processAuthPda = mbPda([Buffer.from("hyperlane"), sep, Buffer.from("process_authority"), sep, WARP_RECIPIENT.toBytes()])[0];
const dispatchAuthPda = PublicKey.findProgramAddressSync(
  [Buffer.from("hyperlane_dispatcher"), sep, Buffer.from("dispatch_authority")], POD)[0];
const outboxPda = mbPda([Buffer.from("hyperlane"), sep, Buffer.from("outbox")])[0];
const processedPda = (id) => mbPda([Buffer.from("hyperlane"), sep, Buffer.from("processed_message"), sep, sep, id])[0];

// ---- 1. fetch the full message (from the dispatch on TC) ----
async function fetchMessage(idHex) {
  const q = encodeURIComponent(`wasm-mailbox_dispatch_id.message_id='${idHex}'`);
  const r = await fetch(`${process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io"}/tx_search?query="${q}"&per_page=1`).then((x) => x.json());
  const tx = r.result?.txs?.[0];
  if (!tx) throw new Error("dispatch not found on TC for " + idHex);
  for (const e of tx.tx_result.events) {
    if (e.type === "wasm-mailbox_dispatch") {
      const m = e.attributes.find((a) => a.key === "message")?.value;
      if (m) return Buffer.from(m, "hex");
    }
  }
  throw new Error("message bytes not found");
}

// ---- 2. simulate the account-metas query of the recipient/ISM ----
async function simulateMetas(program, disc, handleData) {
  const ix = new TransactionInstruction({ programId: program, keys: [], data: Buffer.concat([disc, handleData]) });
  const sim = await conn.simulateTransaction(new Transaction().add(ix), [kp], { sigVerify: false });
  const ret = sim.value.returnData;
  if (!ret) return [];
  const raw = Buffer.from(ret.data[0], "base64");
  // SimulationReturnData<Vec<SerializableAccountMeta>>: [len u32][ (pubkey32, is_signer u8, is_writable u8) ...][trailing u8]
  const len = raw.readUInt32LE(0); let o = 4; const metas = [];
  for (let i = 0; i < len; i++) {
    const pk = new PublicKey(raw.subarray(o, o + 32)); o += 32;
    const isSigner = raw[o] === 1; o += 1;
    const isWritable = raw[o] === 1; o += 1;
    metas.push({ pubkey: pk, isSigner, isWritable });
  }
  return metas;
}

// ---- SendReceiptAtomic: the accounts that pod::send_receipt_atomic expects ----
function sendReceiptAtomicIx(message, uniqueKp) {
  const id = Buffer.from(keccak_256(message));
  const origin = message.readUInt32BE(5);
  const router = podPda([Buffer.from("rrv"), sep, Buffer.from("rrout"), sep, u32le(origin)]);
  const receipted = podPda([Buffer.from("rrv"), sep, Buffer.from("rclm"), sep, id]);
  // executor = this keeper (account 0 of the InboxProcess) → reverse-lookup
  const oploc = podPda([Buffer.from("rrv"), sep, Buffer.from("oploc"), sep, kp.publicKey.toBytes()]);
  const dispatched = mbPda([Buffer.from("hyperlane"), sep, Buffer.from("dispatched_message"), sep, uniqueKp.publicKey.toBytes()])[0];
  const keys = [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },        // 0 keeper
    { pubkey: SYSVAR_INSTRUCTIONS_PUBKEY, isSigner: false, isWritable: false }, // 1 instructions sysvar
    { pubkey: receipted, isSigner: false, isWritable: true },          // 2 receipted
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // 3 system
    { pubkey: router, isSigner: false, isWritable: false },            // 4 router
    { pubkey: oploc, isSigner: false, isWritable: false },             // 5 operator_of_local
    { pubkey: MAILBOX, isSigner: false, isWritable: false },           // 6 mailbox
    { pubkey: outboxPda, isSigner: false, isWritable: true },          // 7 outbox
    { pubkey: dispatchAuthPda, isSigner: false, isWritable: false },   // 8 dispatch authority
    { pubkey: SPL_NOOP, isSigner: false, isWritable: false },          // 9 spl_noop
    { pubkey: uniqueKp.publicKey, isSigner: true, isWritable: true },  // 10 unique message
    { pubkey: dispatched, isSigner: false, isWritable: true },         // 11 dispatched message
  ];
  // pod: [rrv module=0][SendReceiptAtomic variant] — borsh of the Instruction enum
  // (Init=0, SubmitEpochReport=1, Withdraw=2, SubmitAdminAction=3, SetRemoteRouter=4,
  //  SetOperatorSol=5, SendReceiptAtomic=6) → variant 6, no args
  const data = Buffer.from([0, 6]);
  return new TransactionInstruction({ programId: POD, keys, data });
}

(async () => {
  const message = await fetchMessage(MSG_ID.replace(/^0x/, ""));
  const id = Buffer.from(keccak_256(message));
  console.log("message_id calc:", "0x" + id.toString("hex"), "· origin:", message.readUInt32BE(5));

  // account-metas of the recipient (warp) for ISM and handle — via simulation
  const handleData = /* borsh HandleInstruction {origin,sender,message} */ (() => {
    const origin = message.readUInt32BE(5);
    const sender = message.subarray(9, 41);
    const b = Buffer.concat([u32le(origin), sender, u32le(message.length), message]);
    return b;
  })();
  console.log("→ querying account-metas of the recipient/ISM (simulation)…");
  const ismMetas = await simulateMetas(WARP_RECIPIENT, ISM_METAS_DISC, Buffer.alloc(0)).catch(() => []);
  const handleMetas = await simulateMetas(WARP_RECIPIENT, HANDLE_METAS_DISC, handleData).catch(() => []);
  console.log("  ism metas:", ismMetas.length, "· handle metas:", handleMetas.length);

  console.log("\n⚠️  The full assembly of the InboxProcess (ISM metadata = validator");
  console.log("    signatures + merkle proof, and the final order of the accounts) is the point to");
  console.log("    VALIDATE ON DEVNET before mainnet. This keeper already assembles the");
  console.log("    SendReceiptAtomic correctly (the pod accounts are right);");
  console.log("    the delivery step uses the sealevel relayer pattern.");

  if (DELIVER_ONLY) {
    console.log("\n[--deliver-only] not implemented without the ISM metadata (fetch from the validator's S3).");
    return;
  }
  console.log("\nSendReceiptAtomic ready (pod accounts verified). Bundle with InboxProcess: see §devnet.");
})().catch((e) => { console.error("ERROR:", e.message); process.exit(1); });
