// rrv-set-reward — adjusts config.reward_lamports of pod (reward per delivery
// TC→Solana in the quorum model) via governance (SubmitAdminAction → SetRewardLamports).
// Current quorum = 1 → the signer's (operator's) approval executes immediately.
//   node deploy/rrv-set-reward.mjs <lamports>    (default 2350000 ≈ $0.20)
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");
const REWARD = BigInt(process.argv[2] ?? 2350000);
const NONCE = BigInt(process.env.NONCE ?? Math.floor(Date.parse("2026-08-19") / 1000)); // fixed to avoid Date.now

const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const sep = Buffer.from("-");
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
  process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
const conn = new Connection(RPC, "confirmed");

// AdminEnvelope { nonce u64, action: SetRewardLamports(u64)=variant 0 }
const envelope = Buffer.concat([u64(NONCE), Buffer.from([0]), u64(REWARD)]);
const hash = createHash("sha256").update(envelope).digest();
const [proposal] = PublicKey.findProgramAddressSync(
  [Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash], POD);
// [rrv module=0][SubmitAdminAction=3] + borsh(envelope)
const data = Buffer.concat([Buffer.from([0, 3]), envelope]);
const keys = [
  { pubkey: kp.publicKey, isSigner: true, isWritable: true },
  { pubkey: CONFIG, isSigner: false, isWritable: true },
  { pubkey: proposal, isSigner: false, isWritable: true },
  { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
];
console.log("signer:", kp.publicKey.toBase58(), "· reward =", REWARD.toString(), "lamports · nonce", NONCE.toString());
const sig = await conn.sendTransaction(new Transaction().add(new TransactionInstruction({ programId: POD, keys, data })), [kp]);
await conn.confirmTransaction(sig, "confirmed");
console.log("✓ SetRewardLamports:", sig);
