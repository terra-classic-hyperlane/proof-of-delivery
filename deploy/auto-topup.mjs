// auto-topup — tops up the gas trigger wallets when they fall below the
// threshold, drawing from a RESERVE (which you top up now and then).
//
// SAFETY (it moves money on its own — safeguards built in):
//   • only acts when balance < threshold; sends a FIXED amount each time (topup).
//   • reserve FLOOR: never sends if it would leave the reserve below reserveFloor.
//   • per-wallet COOLDOWN (state file): at most 1 topup every COOLDOWN_H hours
//     — if something drains the trigger, it won't empty the reserve in a loop.
//   • DRY by default for testing; only sends with --run.
//
//   usage:
//     node deploy/auto-topup.mjs            # DRY: shows what it would do
//     node deploy/auto-topup.mjs --run      # executes the due topups
//   keys (env, RESERVE wallet — not the triggers):
//     RESERVE_EVM_KEY=0x…   (a single one works for both BNB on BSC and ETH on Ethereum)
//     SOLANA_KEYPAIR=/…json (SOL reserve; default BirXd4Q, which already has balance)
//   RPCs: BSC_RPC / ETH_RPC / SOLANA_RPC (rpc.env). COOLDOWN_H (default 6).
import fs from "node:fs";
import { ethers } from "ethers";
import { Connection, Keypair, PublicKey, SystemProgram, Transaction } from "@solana/web3.js";

const RUN = process.argv.includes("--run");
const COOLDOWN_H = Number(process.env.COOLDOWN_H ?? 6);
const STATE = new URL("./.auto-topup.json", import.meta.url).pathname;
const state = fs.existsSync(STATE) ? JSON.parse(fs.readFileSync(STATE, "utf8")) : {};
const now = Math.floor(Date.now() / 1000);
const log = (...a) => console.log(...a);

const BSC_RPC = process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org";
const ETH_RPC = process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com";
const SOL_RPC = process.env.SOLANA_RPC ?? "https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42";

// wallets to keep funded. threshold/topup/floor in coin UNITS.
const TARGETS = [
  { id: "bsc-trigger", kind: "evm", rpc: BSC_RPC, sym: "BNB",
    addr: "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291", threshold: 0.01, topup: 0.03, reserveFloor: 0.02 },
  { id: "eth-operator", kind: "evm", rpc: ETH_RPC, sym: "ETH",
    addr: "0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae", threshold: 0.005, topup: 0.01, reserveFloor: 0.01 },
  { id: "sol-pbeo", kind: "sol", rpc: SOL_RPC, sym: "SOL",
    addr: "PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS", threshold: 0.02, topup: 0.05, reserveFloor: 0.3 },
];

const canAct = (id) => !state[id] || now - state[id] >= COOLDOWN_H * 3600;
const mark = (id) => { state[id] = now; fs.writeFileSync(STATE, JSON.stringify(state, null, 1)); };

async function doEvm(t, evmKey) {
  const provider = new ethers.JsonRpcProvider(t.rpc);
  const bal = Number(await provider.getBalance(t.addr)) / 1e18;
  if (bal >= t.threshold) { log(`  ${t.id}: ${bal.toFixed(6)} ${t.sym} ≥ ${t.threshold} — ok`); return; }
  if (!canAct(t.id)) { log(`  ${t.id}: LOW (${bal.toFixed(6)}) but in cooldown (≤${COOLDOWN_H}h) — skipping`); return; }
  if (!evmKey) { log(`  ${t.id}: LOW (${bal.toFixed(6)}) — ⚠ missing RESERVE_EVM_KEY`); return; }
  const wallet = new ethers.Wallet(evmKey, provider);
  const rbal = Number(await provider.getBalance(wallet.address)) / 1e18;
  if (rbal - t.topup < t.reserveFloor) { log(`  ${t.id}: reserve ${rbal.toFixed(6)} ${t.sym} insufficient (floor ${t.reserveFloor}) — ⚠ top up the reserve ${wallet.address}`); return; }
  log(`  ${t.id}: LOW ${bal.toFixed(6)} → send ${t.topup} ${t.sym} (reserve ${rbal.toFixed(4)})`);
  if (!RUN) return;
  const legacy = t.rpc.includes("bsc");
  const tx = await wallet.sendTransaction({ to: t.addr, value: ethers.parseEther(String(t.topup)),
    ...(legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {}) });
  log(`    tx ${tx.hash} …`); await tx.wait(); mark(t.id); log(`    ✓ topup confirmed`);
}

async function doSol(t) {
  const conn = new Connection(t.rpc, "confirmed");
  const bal = (await conn.getBalance(new PublicKey(t.addr))) / 1e9;
  if (bal >= t.threshold) { log(`  ${t.id}: ${bal.toFixed(4)} SOL ≥ ${t.threshold} — ok`); return; }
  if (!canAct(t.id)) { log(`  ${t.id}: LOW (${bal.toFixed(4)}) but in cooldown — skipping`); return; }
  const kpFile = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";
  if (!fs.existsSync(kpFile)) { log(`  ${t.id}: LOW — ⚠ SOL reserve not found (${kpFile})`); return; }
  const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(kpFile, "utf8"))));
  const rbal = (await conn.getBalance(kp.publicKey)) / 1e9;
  if (rbal - t.topup < t.reserveFloor) { log(`  ${t.id}: reserve ${rbal.toFixed(4)} SOL insufficient (floor ${t.reserveFloor}) — ⚠ top up ${kp.publicKey.toBase58()}`); return; }
  log(`  ${t.id}: LOW ${bal.toFixed(4)} → send ${t.topup} SOL (reserve ${kp.publicKey.toBase58().slice(0,8)}… ${rbal.toFixed(3)})`);
  if (!RUN) return;
  const tx = new Transaction().add(SystemProgram.transfer({ fromPubkey: kp.publicKey, toPubkey: new PublicKey(t.addr), lamports: Math.round(t.topup * 1e9) }));
  const sig = await conn.sendTransaction(tx, [kp]); await conn.confirmTransaction(sig, "confirmed");
  mark(t.id); log(`    ✓ topup ${sig}`);
}

log(`auto-topup ${RUN ? "(EXECUTING)" : "(DRY — use --run to send)"} · cooldown ${COOLDOWN_H}h`);
const evmKey = process.env.RESERVE_EVM_KEY;
for (const t of TARGETS) {
  try {
    if (t.kind === "evm") await doEvm(t, evmKey);
    else if (t.kind === "sol") await doSol(t);
  } catch (e) { log(`  ${t.id}: ✗ ${String(e).slice(0, 120)}`); }
}
log("done.");
