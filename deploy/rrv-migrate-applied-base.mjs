// rrv-migrate-applied-base — MIGRAÇÃO pós-upgrade: seta o applied_base do guard
// de replay (que veio 0 do Config antigo) para (época atual − 256), alinhando a
// janela ao presente. Sem isso, applied_base=0 e toda submissão de época é
// rejeitada com ERR_EPOCH_TOO_FUTURE — o reporter TC→Solana para.
//
// AdminAction::SetAppliedBase(u64) = variante 9. Quórum atual = 1 → a aprovação
// do operador BirXd4Q (que também é a upgrade authority) executa na hora.
// Só AVANÇA a base (monotônico): 0 → 82_487 é válido; nunca retrocede.
//
//   uso:  node deploy/rrv-migrate-applied-base.mjs           # base = época atual − 256
//         node deploy/rrv-migrate-applied-base.mjs <base>    # base explícita
//         DRY=1 node deploy/rrv-migrate-applied-base.mjs     # só mostra, não envia
import fs from "node:fs";
import { createHash } from "node:crypto";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const RPC = process.env.SOLANA_RPC ?? "https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42";
const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
const CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");
const EPOCH_SECS = 21600;
const WINDOW_BITS = 512;
const DRY = process.env.DRY === "1";

const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
  process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
const conn = new Connection(RPC, "confirmed");

const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const sep = Buffer.from("-");
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];

// lê o applied_base atual do Config (offset: pula bump,quorum,reward,edur,paused,
// operators(vec),total_credited) — mas é mais robusto ler os campos em ordem.
function readBase(d) {
  let o = 0; const u8 = () => d[o++]; const rd8 = () => { let v = 0n; for (let i = 0; i < 8; i++) v |= BigInt(d[o + i]) << BigInt(8 * i); o += 8; return v; };
  u8(); u8(); rd8(); rd8(); u8(); // bump quorum reward edur paused
  const n = d.readUInt32LE(o); o += 4 + n * 32; // operators
  rd8(); // total_credited
  return Number(rd8()); // applied_base
}

const nowEpoch = Math.floor(Date.now() / 1000 / EPOCH_SECS);
const target = process.argv[2] ? Number(process.argv[2]) : nowEpoch - WINDOW_BITS / 2;

(async () => {
  const cfg = await conn.getAccountInfo(CONFIG);
  const curBase = readBase(cfg.data);
  console.log("signatário (operador):", kp.publicKey.toBase58());
  console.log("época atual:", nowEpoch, "| applied_base atual:", curBase, "→ novo:", target);
  if (target < curBase) { console.log("❌ novo < atual — SetAppliedBase só avança. Abortado."); process.exit(1); }
  if (target > nowEpoch) { console.log("❌ novo > época atual — não faz sentido. Abortado."); process.exit(1); }

  // AdminEnvelope { nonce u64, action } · action SetAppliedBase = variante 9 { base u64 }
  const nonce = BigInt(process.env.NONCE ?? Math.floor(Date.now() / 1000));
  const envelope = Buffer.concat([u64(nonce), Buffer.from([9]), u64(target)]);
  const hash = createHash("sha256").update(envelope).digest();
  const proposal = pda([Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash]);
  // [módulo rrv=0][SubmitAdminAction=3] + borsh(envelope) · contas [signer w, config w, proposal w, system]
  const ix = new TransactionInstruction({
    programId: POD,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: CONFIG, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0, 3]), envelope]),
  });
  if (DRY) { console.log("DRY — enviaria SetAppliedBase(", target, ") nonce", nonce.toString()); return; }
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  console.log("✓ SetAppliedBase:", sig);
  const after = readBase((await conn.getAccountInfo(CONFIG)).data);
  console.log(after === target ? `✅ applied_base agora = ${after} — migração OK, reporter liberado` : `⚠️ applied_base = ${after} (esperado ${target}) — CONFERIR`);
})();
