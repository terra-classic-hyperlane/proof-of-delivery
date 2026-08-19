// ⚠️ DESCONTINUADO — modelo ANTIGO (ClaimRemote/atestação, com SetRemoteBinding).
//   O corredor Solana→TC agora usa o MODELO DE RECIBO: veja
//   `deploy/rrv-receipt-config-solana.mjs` (+ `tc-receipt-config-solana.sh`) e §G
//   de `docs/RECIBO-TRUSTLESS.md`. Mantido só como referência histórica.
//
// v2 ClaimRemote na Solana: SetRemoteReward + SetRemoteBinding via proposta
// administrativa (quórum atual 1 → a aprovação do signatário executa).
//   node deploy/rrv-remote-config.mjs
// Assina: BirXd4Q… (operador; keypair local). LOCAL — nada disso roda na VPS.
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const RRV_CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");
const OPERADOR_SOL = new PublicKey("PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS");
const DOM_TC = 132556;
const REWARD = 499000n; // lamports — taxa real medida (tx 4wiG4TtZ…: IGP recebeu 499000)
const BINDING = "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp";

const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
  process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
const conn = new Connection(RPC, "confirmed");
console.log("signatário:", kp.publicKey.toBase58());

const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const str = (s) => Buffer.concat([u32(Buffer.byteLength(s)), Buffer.from(s)]);
const sep = Buffer.from("-");
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];

const rewardPda = pda([Buffer.from("rrv"), sep, Buffer.from("rrew"), sep, u32(DOM_TC)]);
const bindingPda = pda([Buffer.from("rrv"), sep, Buffer.from("rbind"), sep, u32(DOM_TC), sep, Buffer.from(OPERADOR_SOL.toBytes())]);
console.log("reward PDA:", rewardPda.toBase58(), "· binding PDA:", bindingPda.toBase58());

async function adminExec(label, envelope, extra) {
  const hash = createHash("sha256").update(envelope).digest();
  const proposal = pda([Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash]);
  const ix = new TransactionInstruction({
    programId: POD,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: RRV_CONFIG, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: extra, isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.from([0, 3]), envelope]), // [módulo rrv][SubmitAdminAction]
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  console.log(`✓ ${label}:`, sig);
}

// AdminAction::SetRemoteReward = variante 7 { domain u32, reward u64 }
await adminExec("SetRemoteReward(132556, 499000)",
  Buffer.concat([u64(20), Buffer.from([7]), u32(DOM_TC), u64(REWARD)]), rewardPda);
// AdminAction::SetRemoteBinding = variante 8 { domain, operator, remote_address }
await adminExec("SetRemoteBinding(132556, PbEo → terra1run9wz…)",
  Buffer.concat([u64(21), Buffer.from([8]), u32(DOM_TC), Buffer.from(OPERADOR_SOL.toBytes()), str(BINDING)]), bindingPda);
console.log("v2 Solana configurada 🎉");
