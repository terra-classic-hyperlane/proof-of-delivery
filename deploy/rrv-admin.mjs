// Proposta administrativa do VAULT Solana (módulo rrv do pod) — spec §09:
// cada operador submete o MESMO AdminEnvelope; ao atingir o quórum, executa.
//   node deploy/rrv-admin.mjs set-quorum <n> [nonce]
// Chave: SOLANA_KEYPAIR (arquivo JSON) OU SOLANA_SEED_HEX (seed 32B hex).
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const RRV_CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");

const [cmd, valueArg, nonceArg] = process.argv.slice(2);
if (cmd !== "set-quorum") { console.error("uso: rrv-admin.mjs set-quorum <n> [nonce]"); process.exit(1); }
const nonce = BigInt(nonceArg ?? "1");

const kp = process.env.SOLANA_SEED_HEX
  ? Keypair.fromSeed(Uint8Array.from(Buffer.from(process.env.SOLANA_SEED_HEX.replace(/^0x/, ""), "hex")))
  : Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
      process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));

const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
// AdminEnvelope { nonce u64, action: AdminAction::SetQuorum(u8)=variante 1 }
const envelope = Buffer.concat([u64(nonce), Buffer.from([1, Number(valueArg)])]);
const hash = createHash("sha256").update(envelope).digest();
const sep = Buffer.from("-");
const [proposal] = PublicKey.findProgramAddressSync(
  [Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash], POD);

console.log("operador:", kp.publicKey.toBase58(), "· proposta:", proposal.toBase58(), "· envelope sha256:", hash.toString("hex"));

const conn = new Connection(RPC, "confirmed");
const ix = new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: RRV_CONFIG, isSigner: false, isWritable: true },
    { pubkey: proposal, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  // pod: [module=0 rrv][variant=3 SubmitAdminAction][envelope]
  data: Buffer.concat([Buffer.from([0, 3]), envelope]),
});
const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
await conn.confirmTransaction(sig, "confirmed");
console.log("✓ aprovação registrada:", sig);
