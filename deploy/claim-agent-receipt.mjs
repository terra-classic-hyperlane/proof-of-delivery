// claim-agent-receipt — emite os recibos automaticamente em TODAS as chains vivas.
//
// Modelo de RECIBO (não o antigo de atestação). Para cada chain de DESTINO, acha as
// entregas feitas PELO operador que ainda não foram pagas, batela por origem e emite:
//   - TC (CosmWasm) `send_receipt`   → paga BSC→TC (BNB) e Solana→TC (SOL)
//   - BSC (EVM)     `sendReceipt`    → paga TC→BSC (LUNC no TC)
//   - ETH (EVM)     `sendReceipt`    → quando o vault do ETH existir (auto-skip)
//
// A comissão sempre cai na chain de ORIGEM; este agente só DISPARA (emite o recibo no
// destino). O relayer NATIVO entrega o recibo de volta e a origem paga sozinha.
//
// Descoberta (só RPC público, sem getLogs): varre os DISPATCHES do TC (tx_search) para
// achar as msgs por rota; confirma a entrega/estado com eth_call (BSC) ou query (TC).
// Exclui os próprios recibos (recipient == vault) para não "receber comissão de recibo".
//
//   uso:
//     DRY=1 node deploy/claim-agent-receipt.mjs            # só mostra o que faria
//     node deploy/claim-agent-receipt.mjs                  # emite 1 rodada
//     node deploy/claim-agent-receipt.mjs --loop 300       # loop a cada 300s
//   chaves (env): BSC_PRIVATE_KEY (0x…), TC_KEYRING_PASS (senha do keyring)
//   opcionais: TC_KEY (default hyperlane-deploy), *_RPC, DISPATCH_PAGES, MIN_BATCH
import { ethers } from "ethers";
import { spawnSync } from "node:child_process";
import fs from "node:fs";

// estado local persistido: ids já processados (dedup barato entre rodadas). A
// segurança real é on-chain (TC SENT_RECEIPT / remoteClaimed); isto evita gastar
// gás reemitindo — essencial p/ Solana→TC, que não tem query de "já enviado".
const SEEN_FILE = new URL("./.claim-agent-seen.json", import.meta.url).pathname;
let SEEN_IDS = new Set();
try { SEEN_IDS = new Set(JSON.parse(fs.readFileSync(SEEN_FILE, "utf8"))); } catch { /* vazio */ }
function markSeen(ids) {
  ids.forEach((id) => SEEN_IDS.add(id));
  try { fs.writeFileSync(SEEN_FILE, JSON.stringify([...SEEN_IDS])); } catch { /* ignora */ }
}

const DRY = process.env.DRY === "1";
const loopIdx = process.argv.indexOf("--loop");
const LOOP_SECS = loopIdx > -1 ? Number(process.argv[loopIdx + 1] || 300) : 0;
const PAGES = Number(process.env.DISPATCH_PAGES ?? 100); // dispatches recentes p/ varrer
const MIN_BATCH = Number(process.env.MIN_BATCH ?? 1);

const TC = {
  domain: 132556,
  rpc: process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io",
  chainId: "columbus-5",
  vault: "terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q",
  vault32: "402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579",
  mailbox: "terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9",
  operatorTc: "terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp",
  key: process.env.TC_KEY ?? "hyperlane-deploy",
  igp: { 56: "10000000", 1399811149: "10000000" }, // IGP (uluna) por origem do recibo
};
TC.node = TC.rpc + ":443";
const BSC = {
  name: "BSC", domain: 56,
  rpc: process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
  vault: "0x34E06a7793877EC5251b1dC230aD7cD577d231f4",
  vault32: "00000000000000000000000034e06a7793877ec5251b1dc230ad7cd577d231f4",
  mailbox: "0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4",
  operator: "0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291",
};

const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);
const j = (x) => JSON.stringify(x);
const originOf = (hex) => Buffer.from(hex, "hex").readUInt32BE(5);
const recipientOf = (hex) => Buffer.from(hex, "hex").subarray(45, 77).toString("hex");
const keccakId = (hex) => ethers.keccak256("0x" + hex).slice(2);

function terrad(args, stdin) {
  const r = spawnSync("terrad", args, { input: stdin, encoding: "utf8" });
  return (r.stdout || "") + (r.stderr || "");
}
async function tcTxSearch(query, perPage) {
  const url = `${TC.rpc}/tx_search?query=${encodeURIComponent(`"${query}"`)}&per_page=${perPage}&order_by="desc"`;
  try { return (await fetch(url).then((x) => x.json())).result?.txs ?? []; } catch { return []; }
}
async function tcQuery(msg) {
  const out = terrad(["q", "wasm", "contract-state", "smart", TC.vault, j(msg), "--node", TC.node, "--output", "json"]);
  try { return JSON.parse(out).data; } catch { return null; }
}

// Varre os DISPATCHES recentes do mailbox do TC e devolve as msgs de TOKEN (exclui
// recibos, cujo recipient é um vault nosso) por destino.
async function scanTcDispatches() {
  const txs = await tcTxSearch(`wasm-mailbox_dispatch._contract_address='${TC.mailbox}'`, PAGES);
  const out = [];
  for (const t of txs) {
    for (const e of t.tx_result?.events ?? []) {
      if (e.type !== "wasm-mailbox_dispatch") continue;
      const a = Object.fromEntries(e.attributes.map((x) => [x.key, x.value]));
      if (!a.message) continue;
      const msg = a.message;
      const rcpt = recipientOf(msg);
      if (rcpt === TC.vault32 || rcpt === BSC.vault32) continue; // é recibo, não token
      out.push({ id: keccakId(msg), message: msg, dest: Number(a.destination) });
    }
  }
  return out;
}

// ---- TC destino: paga BSC→TC / Solana→TC. Entregas NOSSAS no TC vêm no process tx. ----
async function tcEmit() {
  // o RPC não casa `AND message.sender=…` combinado → varre todos e filtra no código
  const txs = await tcTxSearch(`wasm-mailbox_process._contract_address='${TC.mailbox}'`, PAGES);
  const dels = [];
  for (const t of txs) {
    const out = terrad(["q", "tx", t.hash, "--node", TC.node, "--output", "json"]);
    let tx; try { tx = JSON.parse(out); } catch { continue; }
    for (const m of tx.tx?.body?.messages ?? []) {
      if (m.sender !== TC.operatorTc) continue; // só entregas NOSSAS (signer = operador)
      const msg = m.msg?.process?.message;
      if (!msg) continue;
      if (recipientOf(msg) === TC.vault32) continue; // recibo → não receita
      const origin = originOf(msg);
      if (!(origin in TC.igp)) continue; // só origens com vault/rota
      dels.push({ id: keccakId(msg), message: msg, origin });
    }
  }
  // dedup por origem: BSC→TC é pago no BSC (checável); Solana→TC não tem query
  // ("SENT_RECEIPT" sem query) → confia no estado local + idempotência do send_receipt.
  const bscVaultRO = new ethers.Contract(
    BSC.vault, ["function remoteClaimed(bytes32) view returns (address,uint32,uint256,uint256)"],
    new ethers.JsonRpcProvider(BSC.rpc));
  const pending = [];
  for (const d of dels) {
    if (SEEN_IDS.has(d.id)) continue;
    if (d.origin === 56) {
      try { const rc = await bscVaultRO.remoteClaimed("0x" + d.id); if (rc[0] !== ethers.ZeroAddress) { markSeen([d.id]); continue; } } catch { /* rede */ }
    }
    pending.push(d);
  }
  log(`TC: ${dels.length} entrega(s) nossa(s), ${pending.length} pendente(s)`);
  const byOrigin = {};
  for (const d of pending) (byOrigin[d.origin] ??= []).push(d);
  for (const [origin, ds] of Object.entries(byOrigin)) {
    if (ds.length < MIN_BATCH) continue;
    const messages = ds.map((d) => d.message);
    const amount = TC.igp[origin] ?? "10000000";
    log(`TC send_receipt: origem ${origin}, ${messages.length} id(s) [${ds.map((d) => d.id.slice(0, 10)).join(",")}] IGP ${amount}uluna`);
    if (DRY) continue;
    if (!process.env.TC_KEYRING_PASS) { log("  ⚠ falta TC_KEYRING_PASS"); continue; }
    const out = terrad([
      "tx", "wasm", "execute", TC.vault, j({ send_receipt: { messages } }), "--amount", `${amount}uluna`,
      "--from", TC.key, "--keyring-backend", "file", "--gas", "auto", "--gas-adjustment", "1.5",
      "--gas-prices", "28.325uluna", "--chain-id", TC.chainId, "--node", TC.node,
      "-y", "--output", "json", "--broadcast-mode", "sync",
    ], `${process.env.TC_KEYRING_PASS}\n`);
    const m = out.match(/"txhash":"([0-9A-F]+)"/);
    if (m) markSeen(ds.map((d) => d.id));
    log(`  → ${m ? m[1] : out.trim().slice(0, 160)}`);
  }
}

// ---- EVM destino (BSC): paga TC→BSC. Candidatos = dispatches TC→BSC; confirma com eth_call. ----
const MAILBOX_ABI = ["function processor(bytes32) view returns (address)"];
const VAULT_ABI = [
  "function sendReceipt(bytes[] messages) payable returns (bytes32)",
  "function remoteClaimed(bytes32) view returns (address executor, uint32 domain, uint256 amount, uint256 blockNumber)",
];
async function evmEmit(chain, dispatches) {
  const provider = new ethers.JsonRpcProvider(chain.rpc);
  const mailbox = new ethers.Contract(chain.mailbox, MAILBOX_ABI, provider);
  const vault = new ethers.Contract(chain.vault, VAULT_ABI, provider);
  const cands = dispatches.filter((d) => d.dest === chain.domain);
  const pending = [];
  for (const c of cands) {
    try {
      if (SEEN_IDS.has(c.id)) continue; // já processado (recibo em voo / emitido)
      const proc = await mailbox.processor("0x" + c.id);
      if (proc.toLowerCase() !== chain.operator.toLowerCase()) continue; // não foi nossa entrega
      // TC→BSC: o pagamento é registrado na ORIGEM (TC), não no vault do BSC
      const claimed = (await tcQuery({ remote_claimed: { message_id: c.id } }))?.claimed;
      if (claimed === true) continue; // já pago no TC
      pending.push(c);
    } catch { /* ignora */ }
  }
  log(`${chain.name}: ${cands.length} candidato(s), ${pending.length} pendente(s)`);
  if (pending.length < MIN_BATCH) return;
  log(`${chain.name} sendReceipt: ${pending.length} id(s) [${pending.map((p) => p.id.slice(0, 10)).join(",")}]`);
  if (DRY) return;
  if (!process.env.BSC_PRIVATE_KEY) { log("  ⚠ falta BSC_PRIVATE_KEY"); return; }
  const vw = vault.connect(new ethers.Wallet(process.env.BSC_PRIVATE_KEY, provider));
  try {
    const tx = await vw.sendReceipt(pending.map((p) => "0x" + p.message), { value: 0n });
    log(`  → ${tx.hash} (aguardando)…`); await tx.wait(); log("  ✓ confirmado");
    markSeen(pending.map((p) => p.id));
  } catch (e) { log(`  ✗ ${String(e).slice(0, 140)}`); }
}

async function round() {
  log(`=== rodada ${DRY ? "(DRY)" : ""} ===`);
  const dispatches = await scanTcDispatches();
  try { await tcEmit(); } catch (e) { log("TC erro:", String(e).slice(0, 160)); }
  try { await evmEmit(BSC, dispatches); } catch (e) { log("BSC erro:", String(e).slice(0, 160)); }
  // ETH: quando o vault existir, adicione um objeto igual ao BSC e chame evmEmit(ETH, dispatches).
  log("=== fim ===");
}

if (LOOP_SECS > 0) { log(`loop a cada ${LOOP_SECS}s`); for (;;) { await round(); await new Promise((r) => setTimeout(r, LOOP_SECS * 1000)); } }
else { await round(); }
