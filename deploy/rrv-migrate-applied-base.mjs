// rrv-migrate-applied-base — post-upgrade MIGRATION: sets the applied_base of the
// replay guard (which came in as 0 from the old Config) to (current epoch − 256),
// aligning the window to the present. Without this, applied_base=0 and every epoch
// submission is rejected with ERR_EPOCH_TOO_FUTURE — the TC→Solana reporter stops.
//
// AdminAction::SetAppliedBase(u64) = variant 9. Current quorum = 1 → the approval
// from operator BirXd4Q (which is also the upgrade authority) executes immediately.
// It only ADVANCES the base (monotonic): 0 → 82_487 is valid; never goes backwards.
//
//   usage:  node deploy/rrv-migrate-applied-base.mjs           # base = current epoch − 256
//         node deploy/rrv-migrate-applied-base.mjs <base>    # explicit base
//         DRY=1 node deploy/rrv-migrate-applied-base.mjs     # only shows, does not send
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

// reads the current applied_base from Config (offset: skips bump,quorum,reward,edur,paused,
// operators(vec),total_credited) — but it is more robust to read the fields in order.
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
  console.log("signer (operator):", kp.publicKey.toBase58());
  console.log("current epoch:", nowEpoch, "| current applied_base:", curBase, "→ new:", target);
  if (target < curBase) { console.log("❌ new < current — SetAppliedBase only advances. Aborted."); process.exit(1); }
  if (target > nowEpoch) { console.log("❌ new > current epoch — makes no sense. Aborted."); process.exit(1); }

  // AdminEnvelope { nonce u64, action } · action SetAppliedBase = variant 9 { base u64 }
  const nonce = BigInt(process.env.NONCE ?? Math.floor(Date.now() / 1000));
  const envelope = Buffer.concat([u64(nonce), Buffer.from([9]), u64(target)]);
  const hash = createHash("sha256").update(envelope).digest();
  const proposal = pda([Buffer.from("rrv"), sep, Buffer.from("prop"), sep, hash]);
  // [module rrv=0][SubmitAdminAction=3] + borsh(envelope) · accounts [signer w, config w, proposal w, system]
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
  if (DRY) { console.log("DRY — would send SetAppliedBase(", target, ") nonce", nonce.toString()); return; }
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  console.log("✓ SetAppliedBase:", sig);
  const after = readBase((await conn.getAccountInfo(CONFIG)).data);
  console.log(after === target ? `✅ applied_base now = ${after} — migration OK, reporter unblocked` : `⚠️ applied_base = ${after} (expected ${target}) — CHECK`);
})();
