// igp-tariff — modelo pass-through: o REMETENTE paga a tarifa no IGP da origem
// (~US$ 0,08) e a RECOMPENSA do operador espelha a tarifa do corredor (sem valor
// fixo próprio — "o que o usuário pagou vai pro operador; o lucro é o que sobra").
//
//   --tariff  : ajusta o GÁS COBRADO por corredor (SetGasForDomain no TC ·
//               setGasOracle(gasOverhead) nos IGPs EVM · SetDestinationGasOverheads
//               na Solana) para a cotação do IGP ficar ≈ TARGET_USD hoje.
//               A cotação continua flutuando com gás/câmbio reais (oracle-agent);
//               reajusta rodando de novo.
//   --rewards : grava as recompensas = cotação VIGENTE de cada corredor:
//               TC set_remote_reward[1]/[56] = cotação TC→ETH/TC→BSC (uluna)
//               SOL SetRewardLamports        = cotação TC→SOL convertida p/ lamports
//               BSC setRemoteReward[132556]  = cotação BSC→TC (wei)
//               SOL SetRemoteReward[132556]  = cotação SOL→TC (lamports)
//               (rode DEPOIS de --tariff p/ espelhar a tarifa nova)
//   (sem flag): só mostra cotações e recompensas atuais em $.
//
//   uso:
//     DRY=1 node igp-tariff.mjs --tariff --tc --bsc --eth --sol
//     TC_PRIVATE_KEY=… BSC_PRIVATE_KEY=0x… ETH_PRIVATE_KEY=0x… \
//       node igp-tariff.mjs --tariff --tc --bsc --eth --sol
//     … node igp-tariff.mjs --rewards --tc --bsc --sol
//   TARGET_USD=0.10 muda o alvo (padrão 0.08).
import fs from "node:fs";
import { createHash } from "node:crypto";
import { ethers } from "ethers";
import { CosmWasmClient, SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { GasPrice } from "@cosmjs/stargate";
import { DirectSecp256k1Wallet } from "@cosmjs/proto-signing";

const DRY = process.env.DRY === "1";
const want = (f) => process.argv.includes(f);
const TARGET = Number(process.env.TARGET_USD ?? 0.08);
const log = (...a) => console.log(...a);
const usd = (v) => `$${v.toFixed(4)}`;

async function price(sym) {
  const r = await fetch(`https://api.binance.com/api/v3/ticker/price?symbol=${sym}`).then((x) => x.json());
  if (!r.price) throw new Error(`sem preço p/ ${sym}`);
  return Number(r.price);
}
const P = { LUNC: await price("LUNCUSDT"), BNB: await price("BNBUSDT"), SOL: await price("SOLUSDT"), ETH: await price("ETHUSDT") };
log(`alvo tarifa: ${usd(TARGET)} · LUNC $${P.LUNC} · BNB $${P.BNB} · SOL $${P.SOL} · ETH $${P.ETH}\n`);

// ---------------- TC (origem: TC→ETH / TC→BSC / TC→SOL) ----------------
const TC = {
  rpc: process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io",
  igp: "terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz",
  oracle: "terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d",
  vault: "terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q",
  doms: [1, 56, 1399811149],
};
async function tcSigner(ro) {
  const hex = (process.env.TC_PRIVATE_KEY ?? "").replace(/^0x/, "");
  if (!hex) { log("TC: ⚠ falta TC_PRIVATE_KEY"); return null; }
  const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(hex, "hex")), "terra");
  const sender = (await wallet.getAccounts())[0].address;
  const client = await SigningCosmWasmClient.connectWithSigner(TC.rpc, wallet, { gasPrice: GasPrice.fromString("28.325uluna") });
  return { client, sender };
}
// cotação por unidade de gás (uluna/gas) e gás cobrado por domínio hoje
async function tcState(ro) {
  const out = {};
  const gfd = await ro.queryContractSmart(TC.igp, { igp: { gas_for_domain: { domains: TC.doms } } }).catch(() => null);
  const dflt = Number((await ro.queryContractSmart(TC.igp, { igp: { default_gas: {} } })).gas);
  for (const dom of TC.doms) {
    const o = await ro.queryContractSmart(TC.oracle, { oracle: { get_exchange_rate_and_gas_price: { dest_domain: dom } } });
    const perGas = (Number(o.gas_price) * Number(o.exchange_rate)) / 1e10; // uluna por gás
    const entry = gfd?.gas?.find?.((g) => Number(g[0] ?? g.domain) === dom);
    const gasNow = entry ? Number(entry[1] ?? entry.gas) : dflt;
    out[dom] = { perGas, gasNow, quoteNow: gasNow * perGas };
  }
  return out;
}
async function tcTariff() {
  const ro = await CosmWasmClient.connect(TC.rpc);
  const st = await tcState(ro);
  const targetUluna = (TARGET / P.LUNC) * 1e6;
  const cfg = [];
  for (const dom of TC.doms) {
    const { perGas, gasNow, quoteNow } = st[dom];
    const gasTarget = Math.max(1, Math.round(targetUluna / perGas));
    log(`TC→${dom}: cobra ${gasNow} gás → quote ${Math.round(quoteNow)} uluna (${usd((quoteNow / 1e6) * P.LUNC)}) · novo gás ${gasTarget} → ${usd(TARGET)}`);
    cfg.push([dom, gasTarget.toString()]); // u128 no cw = string JSON (número dá InvalidType)
  }
  if (DRY) return;
  const s = await tcSigner(ro); if (!s) return;
  const res = await s.client.execute(s.sender, TC.igp, { set_gas_for_domain: { config: cfg } }, "auto");
  log(`TC: ✓ SetGasForDomain tx ${res.transactionHash}`);
}
async function tcRewards() {
  const ro = await CosmWasmClient.connect(TC.rpc);
  const st = await tcState(ro);
  if (!DRY) {
    const s = await tcSigner(ro); if (!s) return;
    for (const dom of [1, 56]) {
      const reward = Math.round(st[dom].quoteNow).toString();
      const res = await s.client.execute(s.sender, TC.vault, { set_remote_reward: { domain: dom, reward } }, "auto");
      log(`TC: ✓ remote_reward[${dom}] = ${reward} uluna (${usd((Number(reward) / 1e6) * P.LUNC)}) tx ${res.transactionHash}`);
    }
  } else {
    for (const dom of [1, 56]) log(`TC: remote_reward[${dom}] ← ${Math.round(st[dom].quoteNow)} uluna (${usd((st[dom].quoteNow / 1e6) * P.LUNC)})`);
  }
  // TC→SOL: recompensa é paga em SOL na Solana (reward_lamports) = mesma tarifa em $
  const lam = BigInt(Math.round(((st[1399811149].quoteNow / 1e6) * P.LUNC / P.SOL) * 1e9));
  log(`TC→SOL: tarifa ${usd((st[1399811149].quoteNow / 1e6) * P.LUNC)} → reward_lamports ${lam} (aplicado na fase --sol)`);
  return lam;
}

// ---------------- EVM (origem: BSC→TC / ETH→TC) ----------------
const EVM = {
  bsc: { name: "BSC", keyEnv: "BSC_PRIVATE_KEY", legacy: true, price: () => P.BNB, dec: 1e18,
    rpc: process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
    igp: "0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923", warp: "0x3605d8946fC6f5a75D89D92173100F59743b5318",
    vault: "0x34E06a7793877EC5251b1dC230aD7cD577d231f4" },
  eth: { name: "ETH", keyEnv: "ETH_PRIVATE_KEY", legacy: false, price: () => P.ETH, dec: 1e18,
    rpc: process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com",
    igp: "0x9650F1f8DB492750323172145e67Df4e89E964Aa", warp: "0xA687a4C4Ca49795999b36fDC8A18D1ddD63EdfB5",
    vault: null },
};
const IGP_ABI = [
  "function gasOracle() view returns (address)",
  "function gasOverhead() view returns (uint96)",
  "function setGasOracle(address,uint96)",
  "function owner() view returns (address)",
];
const WARP_Q_ABI = ["function quoteGasPayment(uint32) view returns (uint256)"];
const ORACLE_Q_ABI = ["function getExchangeRateAndGasPrice(uint32) view returns (uint128,uint128)"];
async function evmQuote(c, provider) { // cotação vigente que o remetente paga (via warp)
  return new ethers.Contract(ethers.getAddress(c.warp.toLowerCase()), WARP_Q_ABI, provider).quoteGasPayment(132556);
}
async function evmTariff(c) {
  const provider = new ethers.JsonRpcProvider(c.rpc);
  const igp = new ethers.Contract(ethers.getAddress(c.igp.toLowerCase()), IGP_ABI, provider);
  const [oracle, overheadNow, quoteNow] = await Promise.all([igp.gasOracle(), igp.gasOverhead(), evmQuote(c, provider)]);
  const [rate, gasPrice] = await new ethers.Contract(oracle, ORACLE_Q_ABI, provider).getExchangeRateAndGasPrice(132556);
  const perGasWei = Number((gasPrice * rate) / 10n ** 10n); // wei por unidade de gás
  const totalNow = Math.round(Number(quoteNow) / perGasWei);
  const intrinsic = Math.max(0, totalNow - Number(overheadNow)); // gasLimit que o warp passa (hoje 0)
  const targetWei = (TARGET / c.price()) * c.dec;
  const overheadTarget = Math.max(0, Math.round(targetWei / perGasWei) - intrinsic);
  log(`${c.name}→TC: quote hoje ${usd((Number(quoteNow) / c.dec) * c.price())} (gás ${totalNow}, overhead ${overheadNow}) → overhead ${overheadTarget} p/ ${usd(TARGET)}`);
  if (DRY) return;
  const pk = process.env[c.keyEnv];
  if (!pk) { log(`${c.name}: ⚠ falta ${c.keyEnv}`); return; }
  const w = new ethers.Wallet(pk, provider);
  if (w.address.toLowerCase() !== (await igp.owner()).toLowerCase()) { log(`${c.name}: ⚠ ${w.address} não é o owner do IGP — pulando`); return; }
  const opts = c.legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {};
  const tx = await new ethers.Contract(igp.target, IGP_ABI, w).setGasOracle(oracle, overheadTarget, opts);
  log(`${c.name}: setGasOracle tx ${tx.hash} …`); await tx.wait();
  log(`${c.name}: ✓ quote agora ${usd((Number(await evmQuote(c, provider)) / c.dec) * c.price())}`);
}
async function bscRewards() {
  const c = EVM.bsc;
  const provider = new ethers.JsonRpcProvider(c.rpc);
  const quote = await evmQuote(c, provider);
  log(`BSC: remoteReward[132556] ← ${quote} wei (${usd((Number(quote) / 1e18) * P.BNB)})`);
  if (DRY) return;
  const pk = process.env[c.keyEnv];
  if (!pk) { log("BSC: ⚠ falta BSC_PRIVATE_KEY"); return; }
  const w = new ethers.Wallet(pk, provider);
  const V_ABI = ["function setRemoteReward(uint32,uint256)", "function owner() view returns (address)"];
  const v = new ethers.Contract(ethers.getAddress(c.vault.toLowerCase()), V_ABI, w);
  if (w.address.toLowerCase() !== (await v.owner()).toLowerCase()) { log("BSC: ⚠ não é owner do vault"); return; }
  const tx = await v.setRemoteReward(132556, quote, { gasPrice: (await provider.getFeeData()).gasPrice });
  log(`BSC: setRemoteReward tx ${tx.hash} …`); await tx.wait(); log("BSC: ✓");
}

// ---------------- Solana (origem: SOL→TC · e recompensas do pod) ----------------
async function solPieces() {
  const { Connection, PublicKey } = await import("@solana/web3.js");
  const conn = new Connection(process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com", "confirmed");
  const IGP_INNER = new PublicKey("FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk");
  const OVERHEAD = new PublicKey("FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ");
  const dom = Buffer.alloc(4); dom.writeUInt32LE(132556);
  // varre o IGP inner pelo RemoteGasData do 132556 (mesma técnica do solana-init)
  const d = (await conn.getAccountInfo(IGP_INNER)).data;
  let rate = 0n, gasPrice = 0n, dec = 6;
  for (let i = 0; i + 41 <= d.length; i++) {
    if (d.subarray(i, i + 4).equals(dom) && d[i + 4] === 0) {
      const rd = (off) => { let v = 0n; for (let k = 15; k >= 0; k--) v = (v << 8n) | BigInt(d[off + k]); return v; };
      rate = rd(i + 5); gasPrice = rd(i + 21); dec = d[i + 37];
      if (rate > 0n && gasPrice > 0n && dec === 6) break;
    }
  }
  // overhead atual: varre o overhead igp pelo par (dom u32, u64)
  const od = (await conn.getAccountInfo(OVERHEAD)).data;
  let overheadNow = 0n;
  for (let i = 0; i + 12 <= od.length; i++) {
    if (od.subarray(i, i + 4).equals(dom)) {
      let v = 0n; for (let k = 7; k >= 0; k--) v = (v << 8n) | BigInt(od[i + 4 + k]);
      if (v > 0n && v < 100_000_000n) { overheadNow = v; break; }
    }
  }
  const perGasLam = (Number(gasPrice) * Number(rate)) / 1e19 * 1e3; // lamports por gás (dec 6→9)
  return { conn, OVERHEAD, perGasLam, overheadNow, rate, gasPrice };
}
async function solTariff() {
  const { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } = await import("@solana/web3.js");
  const { conn, OVERHEAD, perGasLam, overheadNow } = await solPieces();
  const INTRINSIC = 3_000_000; // gás que o warp paga além do overhead (tx real: 6M com overhead 3M)
  const targetLam = (TARGET / P.SOL) * 1e9;
  const totalTarget = Math.round(targetLam / perGasLam);
  const overheadTarget = BigInt(Math.max(0, totalTarget - INTRINSIC));
  const quoteNow = (INTRINSIC + Number(overheadNow)) * perGasLam;
  log(`SOL→TC: quote hoje ~${Math.round(quoteNow)} lamports (${usd((quoteNow / 1e9) * P.SOL)}, overhead ${overheadNow}) → overhead ${overheadTarget} p/ ${usd(TARGET)}`);
  if (DRY) return;
  const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
    process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
  const IGP_PROGRAM = new PublicKey("FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR");
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
  const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
  // Instruction::SetDestinationGasOverheads(vec![{domain, Some(overhead)}]) = variante 8 (borsh puro)
  const data = Buffer.concat([Buffer.from([8]), u32(1), u32(132556), Buffer.from([1]), u64(overheadTarget)]);
  const ix = new TransactionInstruction({
    programId: IGP_PROGRAM,
    keys: [
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: OVERHEAD, isSigner: false, isWritable: true },
      { pubkey: kp.publicKey, isSigner: true, isWritable: false },
    ],
    data,
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  log(`SOL: ✓ SetDestinationGasOverheads tx ${sig}`);
}
async function solRewards(rewardLamportsTcToSol) {
  const { Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } = await import("@solana/web3.js");
  const { conn, perGasLam, overheadNow } = await solPieces();
  const INTRINSIC = 3_000_000;
  const quote = BigInt(Math.round((INTRINSIC + Number(overheadNow)) * perGasLam)); // SOL→TC tarifa vigente
  log(`SOL: SetRemoteReward[132556] ← ${quote} lamports (${usd((Number(quote) / 1e9) * P.SOL)})`);
  if (rewardLamportsTcToSol) log(`SOL: SetRewardLamports ← ${rewardLamportsTcToSol} (${usd((Number(rewardLamportsTcToSol) / 1e9) * P.SOL)})`);
  if (DRY) return;
  const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
  const CONFIG = new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w");
  const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(
    process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json", "utf8"))));
  const sep = Buffer.from("-");
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
  const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
  const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];
  const nonce = BigInt(process.env.NONCE ?? Math.floor(Date.now() / 1000));
  async function adminExec(label, envelope, extra) {
    const proposal = pda([Buffer.from("rrv"), sep, Buffer.from("prop"), sep, createHash("sha256").update(envelope).digest()]);
    const keys = [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: CONFIG, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ];
    if (extra) keys.push({ pubkey: extra, isSigner: false, isWritable: true });
    const sig = await conn.sendTransaction(new Transaction().add(new TransactionInstruction({
      programId: POD, keys, data: Buffer.concat([Buffer.from([0, 3]), envelope]),
    })), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    log(`SOL: ✓ ${label}: ${sig}`);
  }
  const rewardPda = pda([Buffer.from("rrv"), sep, Buffer.from("rrew"), sep, u32(132556)]);
  await adminExec(`SetRemoteReward(132556, ${quote})`,
    Buffer.concat([u64(nonce), Buffer.from([7]), u32(132556), u64(quote)]), rewardPda);
  if (rewardLamportsTcToSol) await adminExec(`SetRewardLamports(${rewardLamportsTcToSol})`,
    Buffer.concat([u64(nonce + 1n), Buffer.from([0]), u64(rewardLamportsTcToSol)]));
}

// ---------------- main ----------------
const TARIFF = want("--tariff"), REWARDS = want("--rewards");
if (!TARIFF && !REWARDS) log("(modo leitura — use --tariff e/ou --rewards p/ agir)\n");
if (want("--tc")) { if (TARIFF) await tcTariff(); }
if (want("--bsc") && TARIFF) await evmTariff(EVM.bsc);
if (want("--eth") && TARIFF) await evmTariff(EVM.eth);
if (want("--sol") && TARIFF) await solTariff();
let lamTcSol = null;
if (want("--tc") && (REWARDS || !TARIFF)) lamTcSol = await tcRewards();
if (want("--bsc") && (REWARDS || !TARIFF)) await bscRewards();
if (want("--sol") && (REWARDS || !TARIFF)) await solRewards(lamTcSol);
log("\nfim.");
