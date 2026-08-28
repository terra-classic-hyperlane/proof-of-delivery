// solana-quem-entregou — figures out OFF-CHAIN who delivered a message on Solana.
//
// Path: message_id -> ProcessedMessage PDA -> tx that created it (InboxProcess)
//       -> fee payer of that tx = the relayer/executor.
//
// ⚠️ This is an OFF-CHAIN read (informative): it serves for dashboard, audit and for the
//    operator to know what it delivered. It is NOT a trustless on-chain proof — a contract
//    cannot rely on this (that is the reason why TC->Solana would need a keeper).
//
//   usage:  node deploy/solana-quem-entregou.mjs <message_id_hex> [--rpc <url>]
import { Connection, PublicKey } from "@solana/web3.js";

const MAILBOX = new PublicKey("E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi");
const sep = Buffer.from("-");

const args = process.argv.slice(2);
const idHex = (args.find((a) => !a.startsWith("--")) || "").replace(/^0x/, "");
const RPC = args[args.indexOf("--rpc") + 1] || process.env.SOLANA_RPC || "https://api.mainnet-beta.solana.com";
if (idHex.length !== 64) { console.error("usage: solana-quem-entregou.mjs <message_id_hex(32 bytes)>"); process.exit(1); }

const conn = new Connection(RPC, "confirmed");
const id = Buffer.from(idHex, "hex");

// PDA: ["hyperlane","-","processed_message","-", message_id]
const [pda] = PublicKey.findProgramAddressSync(
  [Buffer.from("hyperlane"), sep, Buffer.from("processed_message"), sep, id],
  MAILBOX,
);

(async () => {
  console.log("message_id :", "0x" + idHex);
  console.log("PDA        :", pda.toBase58());

  const info = await conn.getAccountInfo(pda);
  if (!info) { console.log("\n❌ not yet delivered (PDA does not exist)."); return; }

  // all the signatures that touched the PDA; the OLDEST = the creation = the delivery
  const sigs = await conn.getSignaturesForAddress(pda, { limit: 1000 });
  if (!sigs.length) { console.log("\n⚠️ PDA exists but no signature history on the RPC."); return; }
  const creation = sigs[sigs.length - 1]; // order: most recent -> oldest

  const tx = await conn.getTransaction(creation.signature, { maxSupportedTransactionVersion: 0 });
  const keys = tx.transaction.message.getAccountKeys
    ? tx.transaction.message.getAccountKeys().staticAccountKeys
    : tx.transaction.message.accountKeys;
  const feePayer = keys[0].toBase58(); // account 0 = fee payer = relayer/executor

  console.log("\n✅ delivered");
  console.log("slot       :", creation.slot);
  console.log("delivery tx :", creation.signature);
  console.log("RELAYER    :", feePayer, "  ← who delivered");
  console.log("solscan    : https://solscan.io/tx/" + creation.signature);
})().catch((e) => { console.error("ERROR:", e.message); process.exit(1); });
