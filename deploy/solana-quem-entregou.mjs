// solana-quem-entregou — descobre OFF-CHAIN quem entregou uma mensagem na Solana.
//
// Caminho: message_id -> PDA ProcessedMessage -> tx que a criou (InboxProcess)
//          -> fee payer dessa tx = o relayer/executor.
//
// ⚠️ Isto é leitura OFF-CHAIN (informativa): serve p/ dashboard, auditoria e p/ o
//    operador saber o que entregou. NÃO é prova trustless on-chain — um contrato
//    não pode confiar nisso (é o motivo pelo qual o TC->Solana precisaria de keeper).
//
//   uso:  node deploy/solana-quem-entregou.mjs <message_id_hex> [--rpc <url>]
import { Connection, PublicKey } from "@solana/web3.js";

const MAILBOX = new PublicKey("E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi");
const sep = Buffer.from("-");

const args = process.argv.slice(2);
const idHex = (args.find((a) => !a.startsWith("--")) || "").replace(/^0x/, "");
const RPC = args[args.indexOf("--rpc") + 1] || process.env.SOLANA_RPC || "https://api.mainnet-beta.solana.com";
if (idHex.length !== 64) { console.error("uso: solana-quem-entregou.mjs <message_id_hex(32 bytes)>"); process.exit(1); }

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
  if (!info) { console.log("\n❌ ainda NÃO entregue (PDA não existe)."); return; }

  // todas as assinaturas que tocaram a PDA; a mais ANTIGA = a criação = a entrega
  const sigs = await conn.getSignaturesForAddress(pda, { limit: 1000 });
  if (!sigs.length) { console.log("\n⚠️ PDA existe mas sem histórico de assinaturas no RPC."); return; }
  const creation = sigs[sigs.length - 1]; // ordem: mais recente -> mais antiga

  const tx = await conn.getTransaction(creation.signature, { maxSupportedTransactionVersion: 0 });
  const keys = tx.transaction.message.getAccountKeys
    ? tx.transaction.message.getAccountKeys().staticAccountKeys
    : tx.transaction.message.accountKeys;
  const feePayer = keys[0].toBase58(); // conta 0 = fee payer = relayer/executor

  console.log("\n✅ entregue");
  console.log("slot       :", creation.slot);
  console.log("tx entrega :", creation.signature);
  console.log("RELAYER    :", feePayer, "  ← quem entregou");
  console.log("solscan    : https://solscan.io/tx/" + creation.signature);
})().catch((e) => { console.error("ERRO:", e.message); process.exit(1); });
