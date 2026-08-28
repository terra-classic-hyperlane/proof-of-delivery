// Operator withdrawal (Solana) — WithdrawOperatorSol{index, amount}.
// The receipt SOL is credited to the operator_sol(index) PDA; this script withdraws from it
// to the operator wallet (which MUST be the pubkey registered in SetOperatorSol).
//
//   node deploy/rrv-withdraw-operator.mjs <index> <amount_lamports|all>
//   e.g.:  node deploy/rrv-withdraw-operator.mjs 0 all
//
// Signs with the operator wallet (SOLANA_OP_KEYPAIR). LOCAL — nothing on the VPS.
import fs from "node:fs";
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const INDEX = Number(process.argv[2] ?? 0);
const AMOUNT_ARG = process.argv[3] ?? "all";
// operator wallet (the SAME one registered in SetOperatorSol) — change the path if needed
const KEYPATH = process.env.SOLANA_OP_KEYPAIR
  ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";

const conn = new Connection(RPC, "confirmed");
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(KEYPATH, "utf8"))));
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const sep = Buffer.from("-");
const [opsol] = PublicKey.findProgramAddressSync(
  [Buffer.from("rrv"), sep, Buffer.from("opsol"), sep, u32(INDEX)], POD);

const bal = await conn.getBalance(opsol);
const rentFloor = await conn.getMinimumBalanceForRentExemption(32); // opsol has 32 bytes
const avail = Math.max(0, bal - rentFloor);
const amount = AMOUNT_ARG === "all" ? BigInt(avail) : BigInt(AMOUNT_ARG);
console.log("operator:", kp.publicKey.toBase58(), "· opsol PDA:", opsol.toBase58());
console.log("PDA balance:", bal, "· withdrawable:", avail, "· withdrawing:", amount.toString());
if (amount <= 0n) { console.log("nothing to withdraw."); process.exit(0); }

const ix = new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true }, // signer = registered wallet
    { pubkey: opsol, isSigner: false, isWritable: true },
  ],
  data: Buffer.concat([Buffer.from([0, 6]), u32(INDEX), u64(amount)]), // [rrv][WithdrawOperatorSol]
});
const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
await conn.confirmTransaction(sig, "confirmed");
console.log("✓ withdrawal confirmed:", sig);
