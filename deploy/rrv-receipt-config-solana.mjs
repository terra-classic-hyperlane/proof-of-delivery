// Config do MODELO DE RECIBO no `pod` (Solana) — corredor Solana→TC, SEM keeper.
//
// Registra, no SEU programa (não toca em nada nativo):
//   1. SetRemoteRouter(132556, <vault_TC 32B>)  → o `handle` só aceita recibo vindo
//      do vault do TC (confere sender == router).
//   2. SetOperatorSol(index, <carteira Solana>) → índice do operador (o MESMO do
//      de/para do TC) → PDA que acumula o SOL; a carteira é quem pode sacar.
//   3. (governança, quórum) SetRemoteReward(132556, <lamports>) → quanto o `handle`
//      credita por entrega. Pode já estar setado (PDA de reward é compartilhada).
//
// SetRemoteRouter/SetOperatorSol são gated por operador (single-sig do operador do
// módulo rrv); SetRemoteReward é ação administrativa (proposta + quórum atual = 1).
//
//   node deploy/rrv-receipt-config-solana.mjs
// Assina: BirXd4Q… (operador rrv; keypair local). LOCAL — nada disso roda na VPS.
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

// ---- endereços de produção ----
const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const RRV_CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w"); // config PDA do módulo rrv (= o pool)
const DOM_TC = 132556;

// vault do TC em 32 bytes (bech32 decode) — o router confiável de onde vem o recibo
const VAULT_TC_32 = Buffer.from("402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579", "hex");
// índice do operador (o MESMO do de/para do TC) e a carteira Solana que recebe/saca
const OP_INDEX = Number(process.env.OP_INDEX ?? 0);
const OP_WALLET = new PublicKey(process.env.OP_WALLET ?? "PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS");
const REWARD = BigInt(process.env.REWARD ?? 499000n); // lamports por entrega (taxa de origem medida)
const SET_REWARD = process.env.SKIP_REWARD !== "1"; // já pode estar setado (PDA compartilhada)

const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
  process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
const conn = new Connection(RPC, "confirmed");
console.log("signatário:", kp.publicKey.toBase58());

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

// ---- 1. SetRemoteRouter (rrv variante 4): [signer w, config, router PDA w, system]
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

// ---- 2. SetOperatorSol (rrv variante 5): [signer w, config, opsol PDA w, oploc PDA w, system]
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

// ---- 3. SetRemoteReward (governança: SubmitAdminAction variante 3 → AdminAction 7)
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
console.log("corredor Solana→TC (lado pod) configurado 🎉  — agora rode o lado TC (tc-receipt-config-solana.sh)");
