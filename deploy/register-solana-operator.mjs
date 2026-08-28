// Registers the Solana relayer wallet as OPERATOR of the governor (pod gov module).
// Signature: current multisig (BirXd4Q…, owner of the local keypair).
//   node deploy/register-solana-operator.mjs
// Instruction: pod [module=1][variant=3 SetOperators][add=[PbEo7Fn2…]][remove=[]]
// Accounts: [multisig signer, gov config PDA w]. Quorum stays 1.
import fs from "node:fs";
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const KEYPAIR = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const GOV_CONFIG = new PublicKey("4sZAfqDqEmR7LMWjrdNmoEkv8S6BDdnDkh5mfADenaaA");
const NEW_OP = new PublicKey(process.argv[2] ?? "PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS");

const conn = new Connection(RPC, "confirmed");
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(KEYPAIR, "utf8"))));
console.log("multisig signer:", kp.publicKey.toBase58(), "· new operator:", NEW_OP.toBase58());

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const data = Buffer.concat([Buffer.from([1, 3]), u32(1), Buffer.from(NEW_OP.toBytes()), u32(0)]);
const ix = new TransactionInstruction({
  programId: POD,
  keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    { pubkey: GOV_CONFIG, isSigner: false, isWritable: true },
  ],
  data,
});
const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
await conn.confirmTransaction(sig, "confirmed");
console.log("✓ SetOperators(add) confirmed:", sig);
