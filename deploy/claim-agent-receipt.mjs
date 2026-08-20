// claim-agent-receipt — emite os recibos automaticamente (TC↔BSC + Solana→TC).
//
// SEM terrad (roda na VPS só com node): TC via cosmjs + LCD/RPC; BSC via ethers.
// Assina com uma CARTEIRA-GATILHO dedicada (só paga o gás) — a comissão sempre cai na
// carteira de OPERADOR do registro de/para, não em quem assina. Se a chave-gatilho
// vazar, o atacante só pega o troco de gás.
//
//   uso:
//     DRY=1 node claim-agent-receipt.mjs                 # só leitura
//     node claim-agent-receipt.mjs                       # 1 rodada
//     node claim-agent-receipt.mjs --loop 300            # serviço
//   chaves (env) — carteira-gatilho dedicada:
//     BSC_PRIVATE_KEY=0x…    hex (paga gás BNB no BSC, p/ TC→BSC)
//     TC_PRIVATE_KEY=…       hex de 32 bytes (paga gás LUNC no TC, p/ BSC→TC e Solana→TC)
//       (alternativa: TC_MNEMONIC="12/24 palavras")
//   opcionais: *_RPC, TC_LCD, DISPATCH_PAGES, MIN_BATCH
import { ethers } from "ethers";
import fs from "node:fs";
import { CosmWasmClient, SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { GasPrice } from "@cosmjs/stargate";
import { DirectSecp256k1Wallet, DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";

const DRY = process.env.DRY === "1";
const li = process.argv.indexOf("--loop");
const LOOP_SECS = li > -1 ? Number(process.argv[li + 1] || 300) : 0;
const PAGES = Number(process.env.DISPATCH_PAGES ?? 100);
const MIN_BATCH = Number(process.env.MIN_BATCH ?? 1);

const TC = {
  domain: 132556,
  rpc: process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io",
  lcd: process.env.TC_LCD ?? "https://lcd.terra-classic.hexxagon.io",
  gasPrice: "28.325uluna",
  vault: "terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q",
  vault32: "402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579",
  mailbox: "terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9",
  operatorTc: "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp",
  igpCore: "terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz",
  // gás de ENTREGA do recibo por domínio de origem (metadata do IGP) — o valor
  // em uluna é cotado dinamicamente; nada de tarifa fixa (env RECEIPT_GAS_<dom>)
  receiptGas: {
    56: Number(process.env.RECEIPT_GAS_56 ?? 300000),
    1399811149: Number(process.env.RECEIPT_GAS_SOL ?? 500000),
  },
};
const BSC = {
  name: "BSC", domain: 56,
  rpc: process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
  vault: "0x34E06a7793877EC5251b1dC230aD7cD577d231f4",
  vault32: "00000000000000000000000034e06a7793877ec5251b1dc230ad7cd577d231f4",
  mailbox: "0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4",
  operator: "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291",
};

const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);
const originOf = (hex) => Buffer.from(hex, "hex").readUInt32BE(5);
const recipientOf = (hex) => Buffer.from(hex, "hex").subarray(45, 77).toString("hex");
const keccakId = (hex) => ethers.keccak256("0x" + hex).slice(2);

const SEEN_FILE = new URL("./.claim-agent-seen.json", import.meta.url).pathname;
let SEEN = new Set();
try { SEEN = new Set(JSON.parse(fs.readFileSync(SEEN_FILE, "utf8"))); } catch { /* vazio */ }
const markSeen = (ids) => { ids.forEach((i) => SEEN.add(i)); try { fs.writeFileSync(SEEN_FILE, JSON.stringify([...SEEN])); } catch {} };

// ---- TC leitura (sem terrad) ----
let _cw;
async function cw() { return (_cw ??= await CosmWasmClient.connect(TC.rpc)); }
async function tcQuery(msg) { try { return await (await cw()).queryContractSmart(TC.vault, msg); } catch { return null; } }
async function tcTxSearch(query, per) {
  const url = `${TC.rpc}/tx_search?query=${encodeURIComponent(`"${query}"`)}&per_page=${per}&order_by="desc"`;
  try { return (await fetch(url).then((x) => x.json())).result?.txs ?? []; } catch { return []; }
}
async function tcTxMessages(hash) { // process msgs via LCD (decodificado)
  try {
    const r = await fetch(`${TC.lcd}/cosmos/tx/v1beta1/txs/${hash}`).then((x) => x.json());
    return r.tx?.body?.messages ?? [];
  } catch { return []; }
}

async function scanTcDispatches() {
  const txs = await tcTxSearch(`wasm-mailbox_dispatch._contract_address='${TC.mailbox}'`, PAGES);
  const out = [];
  for (const t of txs) for (const e of t.tx_result?.events ?? []) {
    if (e.type !== "wasm-mailbox_dispatch") continue;
    const a = Object.fromEntries(e.attributes.map((x) => [x.key, x.value]));
    if (!a.message) continue;
    const r = recipientOf(a.message);
    if (r === TC.vault32 || r === BSC.vault32) continue; // recibo, não token
    out.push({ id: keccakId(a.message), message: a.message, dest: Number(a.destination) });
  }
  return out;
}

// ---- TC destino (send_receipt): BSC→TC / Solana→TC ----
async function tcSigner() {
  if (process.env.TC_PRIVATE_KEY) {
    const hex = process.env.TC_PRIVATE_KEY.replace(/^0x/, "");
    return DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(hex, "hex")), "terra");
  }
  if (process.env.TC_MNEMONIC) return DirectSecp256k1HdWallet.fromMnemonic(process.env.TC_MNEMONIC, { prefix: "terra" });
  return null;
}

async function tcEmit() {
  const txs = await tcTxSearch(`wasm-mailbox_process._contract_address='${TC.mailbox}'`, PAGES);
  const dels = [];
  for (const t of txs) for (const m of await tcTxMessages(t.hash)) {
    if (m.sender !== TC.operatorTc) continue; // só entregas NOSSAS
    const msg = m.msg?.process?.message;
    if (!msg) continue;
    if (recipientOf(msg) === TC.vault32) continue;
    const origin = originOf(msg);
    if (!(origin in TC.receiptGas)) continue;
    dels.push({ id: keccakId(msg), message: msg, origin });
  }
  // dedup: BSC→TC pago no BSC (checável); Solana→TC via SEEN local + idempotência
  const bscRO = new ethers.Contract(BSC.vault, ["function remoteClaimed(bytes32) view returns (address,uint32,uint256,uint256)"], new ethers.JsonRpcProvider(BSC.rpc));
  const pend = [];
  for (const d of dels) {
    if (SEEN.has(d.id)) continue;
    if (d.origin === 56) { try { if ((await bscRO.remoteClaimed("0x" + d.id))[0] !== ethers.ZeroAddress) { markSeen([d.id]); continue; } } catch {} }
    pend.push(d);
  }
  log(`TC: ${dels.length} entrega(s), ${pend.length} pendente(s)`);
  const byO = {};
  for (const d of pend) (byO[d.origin] ??= []).push(d);
  const wallet = DRY ? null : await tcSigner();
  let client, sender;
  if (wallet) { client = await SigningCosmWasmClient.connectWithSigner(TC.rpc, wallet, { gasPrice: GasPrice.fromString(TC.gasPrice) }); sender = (await wallet.getAccounts())[0].address; }
  for (const [origin, ds] of Object.entries(byO)) {
    if (ds.length < MIN_BATCH) continue;
    // gás REAL de entrega do recibo no destino (via metadata do IGP) — NUNCA a
    // tarifa cheia de usuário; a cotação em uluna é DINÂMICA (quote na hora).
    const gasLimit = TC.receiptGas[origin] ?? 300000;
    let amount;
    try {
      const q = await (await cw()).queryContractSmart(TC.igpCore, {
        igp: { quote_gas_payment: { dest_domain: Number(origin), gas_amount: String(gasLimit) } },
      });
      amount = ((BigInt(q.gas_needed) * 102n) / 100n).toString(); // +2% (excedente reembolsado)
    } catch (e) { log(`  ✗ quote IGP falhou p/ origem ${origin}: ${String(e).slice(0, 100)}`); continue; }
    log(`TC send_receipt: origem ${origin}, ${ds.length} id(s) [${ds.map((d) => d.id.slice(0, 10)).join(",")}] gás ${gasLimit} → IGP ${amount}uluna`);
    if (DRY) continue;
    if (!wallet) { log("  ⚠ falta TC_PRIVATE_KEY/TC_MNEMONIC"); continue; }
    try {
      const res = await client.execute(sender, TC.vault,
        { send_receipt: { messages: ds.map((d) => d.message), gas_limit: String(gasLimit) } },
        "auto", "", [{ denom: "uluna", amount }]);
      markSeen(ds.map((d) => d.id));
      log(`  → ${res.transactionHash}`);
    } catch (e) { log(`  ✗ ${String(e).slice(0, 140)}`); }
  }
}

// ---- BSC destino (sendReceipt): TC→BSC ----
const MAILBOX_ABI = ["function processor(bytes32) view returns (address)"];
const VAULT_ABI = ["function sendReceipt(bytes[] messages) payable returns (bytes32)"];
async function evmEmit(chain, dispatches) {
  const provider = new ethers.JsonRpcProvider(chain.rpc);
  const mailbox = new ethers.Contract(chain.mailbox, MAILBOX_ABI, provider);
  const cands = dispatches.filter((d) => d.dest === chain.domain);
  const pend = [];
  for (const c of cands) {
    try {
      if (SEEN.has(c.id)) continue;
      if ((await mailbox.processor("0x" + c.id)).toLowerCase() !== chain.operator.toLowerCase()) continue;
      if ((await tcQuery({ remote_claimed: { message_id: c.id } }))?.claimed === true) continue; // pago no TC
      pend.push(c);
    } catch {}
  }
  log(`${chain.name}: ${cands.length} candidato(s), ${pend.length} pendente(s)`);
  if (pend.length < MIN_BATCH) return;
  log(`${chain.name} sendReceipt: ${pend.length} id(s) [${pend.map((p) => p.id.slice(0, 10)).join(",")}]`);
  if (DRY) return;
  if (!process.env.BSC_PRIVATE_KEY) { log("  ⚠ falta BSC_PRIVATE_KEY"); return; }
  const vault = new ethers.Contract(chain.vault, VAULT_ABI, new ethers.Wallet(process.env.BSC_PRIVATE_KEY, provider));
  try {
    const tx = await vault.sendReceipt(pend.map((p) => "0x" + p.message), { value: 0n });
    log(`  → ${tx.hash} (aguardando)…`); await tx.wait(); markSeen(pend.map((p) => p.id)); log("  ✓ confirmado");
  } catch (e) { log(`  ✗ ${String(e).slice(0, 140)}`); }
}

async function whoami() {
  if (process.env.BSC_PRIVATE_KEY) { try { log("carteira-gatilho BSC (mande BNB p/ gás):", new ethers.Wallet(process.env.BSC_PRIVATE_KEY).address); } catch {} }
  const w = await tcSigner(); if (w) { try { log("carteira-gatilho TC (mande LUNC p/ gás):", (await w.getAccounts())[0].address); } catch {} }
}

async function round() {
  log(`=== rodada ${DRY ? "(DRY)" : ""} ===`);
  const disp = await scanTcDispatches();
  try { await tcEmit(); } catch (e) { log("TC erro:", String(e).slice(0, 160)); }
  try { await evmEmit(BSC, disp); } catch (e) { log("BSC erro:", String(e).slice(0, 160)); }
  log("=== fim ===");
}
await whoami();
if (LOOP_SECS > 0) { log(`loop a cada ${LOOP_SECS}s`); for (;;) { await round(); await new Promise((r) => setTimeout(r, LOOP_SECS * 1000)); } }
else { await round(); }
