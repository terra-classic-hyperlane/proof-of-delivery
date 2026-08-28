// Config of the RECEIPT MODEL in `pod` (Solana) — Solana→TC corridor, WITHOUT keeper.
//
// Registers, in YOUR program (does not touch anything native):
//   1. SetRemoteRouter(132556, <vault_TC 32B>)  → `handle` only accepts a receipt coming
//      from the TC vault (checks sender == router).
//   2. SetOperatorSol(index, <Solana wallet>) → operator index (the SAME as the
//      TC mapping) → PDA that accumulates the SOL; the wallet is who can withdraw.
//   3. (governance, quorum) SetRemoteReward(132556, <lamports>) → how much `handle`
//      credits per delivery. May already be set (reward PDA is shared).
//
// SetRemoteRouter/SetOperatorSol are gated by operator (single-sig of the rrv
// module operator); SetRemoteReward is an administrative action (proposal + current quorum = 1).
//
//   node deploy/rrv-receipt-config-solana.mjs
// Signs: BirXd4Q… (rrv operator; local keypair). LOCAL — none of this runs on the VPS.
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

// ---- production addresses ----
const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const RRV_CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w"); // config PDA of the rrv module (= the pool)
const DOM_TC = 132556;

// TC vault as 32 bytes (bech32 decode) — the trusted router the receipt comes from
const VAULT_TC_32 = Buffer.from("402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579", "hex");
// operator index (the SAME as the TC mapping) and the Solana wallet that receives/withdraws
const OP_INDEX = Number(process.env.OP_INDEX ?? 0);
const OP_WALLET = new PublicKey(process.env.OP_WALLET ?? "PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS");
const REWARD = BigInt(process.env.REWARD ?? 499000n); // lamports per delivery (measured origin fee)
const SET_REWARD = process.env.SKIP_REWARD !== "1"; // may already be set (shared PDA)

const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
  process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
const conn = new Connection(RPC, "confirmed");
console.log("signer:", kp.publicKey.toBase58());

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const sep = Buffer.from("-");
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];

const routerPda = pda([Buffer.from("rrv"), sep, Buffer.from("rrout"), sep, u32(DOM_TC)]);
const opsolPda = pda([Buffer.from("rrv"), sep, Buffer.from("opsol"), sep, u32(OP_INDEX)]);
const oplocPda = pda([Buffer.from("rrv"), sep, Buffer.from("oploc"), sep, Buffer.from(OP_WALLET.toBytes())]);
const rewardPda = pda([Buffer.from("rrv"), sep, Buffer.from("rrew"), sep, u32(DOM_TC)]);
console.log("router PDA:", routerPda.toBase58(), "· opsol PDA:", opsolPda.toBase58());

async function send(label, ix) {
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  console.log(`✓ ${label}:`, sig);
}

// ---- 1. SetRemoteRouter (rrv variant 4): [signer w, config, router PDA w, system]
await send(`SetRemoteRouter(${DOM_TC}, vault_TC)`, new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: RRV_CONFIG, isSigner: false, isWritable: false },
    { pubkey: routerPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([0, 4]), u32(DOM_TC), VAULT_TC_32]),
}));

// ---- 2. SetOperatorSol (rrv variant 5): [signer w, config, opsol PDA w, oploc PDA w, system]
await send(`SetOperatorSol(${OP_INDEX} → ${OP_WALLET.toBase58()})`, new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: RRV_CONFIG, isSigner: false, isWritable: false },
    { pubkey: opsolPda, isSigner: false, isWritable: true },
    { pubkey: oplocPda, isSigner: false, isWritable: true },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([Buffer.from([0, 5]), u32(OP_INDEX), Buffer.from(OP_WALLET.toBytes())]),
}));

// ---- 3. SetRemoteReward (governance: SubmitAdminAction variant 3 → AdminAction 7)
if (SET_REWARD) {
  // envelope = borsh(AdminEnvelope { nonce u64, action: SetRemoteReward{domain,reward} })
  const envelope = Buffer.concat([u64(30), Buffer.from([7]), u32(DOM_TC), u64(REWARD)]);
  const hash = createHash("sha256").update(envelope).digest();
  const proposal = pda([Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash]);
  await send(`SetRemoteReward(${DOM_TC}, ${REWARD})`, new TransactionInstruction({
    programId: POD,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: RRV_CONFIG, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: rewardPda, isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.from([0, 3]), envelope]),
  }));
}
console.log("Solana→TC corridor (pod side) configured 🎉  — now run the TC side (tc-receipt-config-solana.sh)");
