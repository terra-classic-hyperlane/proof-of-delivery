// Saque do operador (Solana) — WithdrawOperatorSol{index, amount}.
// O SOL do recibo é creditado na PDA operator_sol(index); este script saca dela
// para a carteira do operador (que TEM de ser o pubkey registrado no SetOperatorSol).
//
//   node deploy/rrv-withdraw-operator.mjs <index> <amount_lamports|all>
//   ex:  node deploy/rrv-withdraw-operator.mjs 0 all
//
// Assina com a carteira do operador (SOLANA_OP_KEYPAIR). LOCAL — nada na VPS.
import fs from "node:fs";
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const INDEX = Number(process.argv[2] ?? 0);
const AMOUNT_ARG = process.argv[3] ?? "all";
// carteira do operador (a MESMA registrada em SetOperatorSol) — troque o caminho se preciso
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
const rentFloor = await conn.getMinimumBalanceForRentExemption(32); // opsol tem 32 bytes
const avail = Math.max(0, bal - rentFloor);
const amount = AMOUNT_ARG === "all" ? BigInt(avail) : BigInt(AMOUNT_ARG);
console.log("operador:", kp.publicKey.toBase58(), "· opsol PDA:", opsol.toBase58());
console.log("saldo PDA:", bal, "· sacável:", avail, "· sacando:", amount.toString());
if (amount <= 0n) { console.log("nada a sacar."); process.exit(0); }

const ix = new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true }, // signer = carteira registrada
    { pubkey: opsol, isSigner: false, isWritable: true },
  ],
  data: Buffer.concat([Buffer.from([0, 6]), u32(INDEX), u64(amount)]), // [rrv][WithdrawOperatorSol]
});
const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
await conn.confirmTransaction(sig, "confirmed");
console.log("✓ saque confirmado:", sig);
