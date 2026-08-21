// deliver-receipts-tc — ENTREGADOR PRIMÁRIO dos recibos BSC→TC no TC.
//
// ⚠️ POR QUE PRIMÁRIO (não o relayer): a conta terra1run9wz é assinada pelo
// relayer, pelo claim-agent e por scripts. O relayer CACHEIA a sequence e falha
// (executed:false, gas_used:0) sempre que outro assina — então NÃO entrega
// recibos no TC de forma confiável. Este agente usa cosmjs, que busca a sequence
// FRESCA a cada assinatura → imune à contenção. Por isso é o entregador primário
// dos recibos BSC→TC (timer de 3 min). O relayer segue primário p/ TC→remota.
//
// Faz o que o relayer faria (sem tocar no core): monta o metadata do ISM a partir
// dos checkpoints públicos em S3 dos validadores OFICIAIS da BSC (4-de-6,
// verificados por message_id) e executa `process` no mailbox do TC via cosmjs.
// Idempotente (mailbox rejeita "already delivered"; o vault deduplica).
//
//   uso:
//     DRY=1 node deliver-receipts-tc.mjs         # mostra pendentes e cobertura
//     node deliver-receipts-tc.mjs               # entrega os presos (>STUCK_MINUTES)
//     node deliver-receipts-tc.mjs --tx 0x…      # ingere um sendReceipt na fila
//     FORCE=1 node deliver-receipts-tc.mjs       # entrega já (ignora a janela)
//   env: TC_PRIVATE_KEY · TC_RPC/TC_LCD/BSC_RPC · STUCK_MINUTES (padrão 3)
import { ethers } from "ethers";
import { SigningCosmWasmClient, CosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { GasPrice } from "@cosmjs/stargate";
import { DirectSecp256k1Wallet } from "@cosmjs/proto-signing";

const DRY = process.env.DRY === "1";
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

const TC = {
  rpc: process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io",
  mailbox: "terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9",
  routingIsm: "terra1uhzzvt9x3u8hjnkp695hklexx2uywjvfqv454d93ds92sgtpwk7qrpxdg0",
};
const BSC = {
  rpc: process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
  mailbox: "0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4",
  vault: "0x34E06a7793877EC5251b1dC230aD7cD577d231f4",
  merkleHook: "0xFDb9Cd5f9daAA2E4474019405A328a88E7484f26",
  va: "0x7024078130D9c2100fEA474DAD009C2d1703aCcd",
  domain: 56,
};
const DISPATCH_TOPIC = "0x769f711d20c679153d382254f59892613b58a97cc876b249134ac25c80f9c814";
const LOOKBACK = Number(process.env.LOOKBACK_BLOCKS ?? 400_000);

// ---- 1. fila local de recibos emitidos (o claim-agent registra ao emitir) ----
// Sem varredura de logs (RPCs públicos da BSC recusam getLogs): fonte = arquivo
// .receipts-tc-pending.json + opção --tx 0x… p/ ingerir um sendReceipt manualmente.
import fs from "node:fs";
const PENDING_FILE = new URL("./.receipts-tc-pending.json", import.meta.url).pathname;
const loadPending = () => { try { return JSON.parse(fs.readFileSync(PENDING_FILE, "utf8")); } catch { return []; } };
const savePending = (l) => fs.writeFileSync(PENDING_FILE, JSON.stringify(l, null, 1));

async function ingestTx(provider, txHash) {
  const r = await provider.getTransactionReceipt(txHash);
  if (!r) throw new Error("tx não encontrada: " + txHash);
  const out = [];
  for (const l of r.logs) {
    if (l.topics[0] !== DISPATCH_TOPIC) continue;
    const d = l.data.slice(2);
    const len = parseInt(d.slice(64, 128), 16);
    const msg = d.slice(128, 128 + len * 2);
    out.push({ msg, id: ethers.keccak256("0x" + msg).slice(2), nonce: parseInt(msg.slice(2, 10), 16), src: txHash, seenAt: nowSec() });
  }
  return out;
}
const nowSec = () => Math.floor(Number(process.env.NOW_SEC ?? Date.now() / 1000));
const STUCK_SECS = Number(process.env.STUCK_MINUTES ?? 3) * 60;

async function pendingReceipts(provider, cw) {
  let list = loadPending();
  const ti = process.argv.indexOf("--tx");
  if (ti > -1) {
    const extra = await ingestTx(provider, process.argv[ti + 1]);
    for (const e of extra) if (!list.some((x) => x.id === e.id)) list.push(e);
    savePending(list);
  }
  const stillPending = [], out = [];
  for (const p of list) {
    const r = await cw.queryContractSmart(TC.mailbox, { mailbox: { message_delivered: { id: p.id } } });
    if (r.delivered) continue; // relayer (ou nós) já entregou — some da fila
    stillPending.push(p);
    // só entra na lista de AÇÃO se preso além da janela (relayer teve a chance)
    const age = nowSec() - (p.seenAt ?? nowSec());
    if (process.env.FORCE === "1" || age >= STUCK_SECS) out.push(p);
    else log(`  ⏳ ${p.id.slice(0, 10)}… preso há ${Math.floor(age / 60)}min (<${STUCK_SECS / 60}) — aguardando o relayer`);
  }
  savePending(stillPending); // preserva seenAt dos que seguem pendentes
  return out;
}

// ---- 2. validadores/threshold exigidos pelo ISM do TC p/ origem BSC ----
async function ismRequirements(cw) {
  const route = await cw.queryContractSmart(TC.routingIsm, { routing_ism: { route: { message: "03" + "00".repeat(4) + "00000038" + "00".repeat(68) } } })
    .catch(() => null);
  // fallback: ISM da BSC conhecido do deploy (guia TC)
  const ism = route?.ism ?? "terra1nqj7qlnt2sty0dgnu3ss5z4u6wr7hjfea7cn6wpwjt2uymts8ucsmuj9xw";
  const info = await cw.queryContractSmart(ism, { ism: { verify_info: { message: "03" + "00".repeat(4) + "00000038" + "00".repeat(68) } } });
  return { validators: info.validators.map((v) => v.toLowerCase().replace(/^0x/, "")), threshold: info.threshold };
}

// ---- 3. checkpoints dos validadores (S3 público, anunciado on-chain) ----
function s3ToHttp(loc) {
  // s3://bucket/region[/prefixo] → https://bucket.s3.region.amazonaws.com[/prefixo]
  const m = loc.match(/^s3:\/\/([^/]+)\/([^/]+)(?:\/(.*))?$/);
  if (!m) return null;
  return `https://${m[1]}.s3.${m[2]}.amazonaws.com${m[3] ? "/" + m[3] : ""}`;
}
// O índice da FOLHA no merkle hook ≠ nonce do mailbox (o hook foi implantado
// depois do mailbox): offset constante, medido em produção = 1525. O arquivo
// checkpoint_{index}_with_id.json carrega o message_id — verificamos e, se
// divergir, procuramos ±10 (auto-corretivo caso o offset mude um dia).
const TREE_OFFSET = Number(process.env.BSC_TREE_OFFSET ?? 1525);
async function findIndex(base, nonce, msgId) {
  const cand = nonce - TREE_OFFSET;
  for (const idx of [cand, ...Array.from({ length: 20 }, (_, k) => cand + (k % 2 ? -1 : 1) * Math.ceil((k + 1) / 2))]) {
    if (idx < 0) continue;
    try {
      const r = await fetch(`${base}/checkpoint_${idx}_with_id.json`).then((x) => x.ok ? x.json() : null);
      if (!r) continue;
      const cid = (r.value?.message_id ?? "").toLowerCase().replace(/^0x/, "");
      if (cid === msgId) return { idx, r };
    } catch { /* tenta próximo */ }
  }
  return null;
}
async function fetchSignatures(provider, validators, threshold, nonce, msgId) {
  const va = new ethers.Contract(BSC.va, ["function getAnnouncedStorageLocations(address[]) view returns (string[][])"], provider);
  const locs = await va.getAnnouncedStorageLocations(validators.map((v) => "0x" + v));
  const found = []; // { root, index, sig }
  let knownIdx = null;
  for (let i = 0; i < validators.length; i++) {
    const urls = (locs[i] ?? []).map(s3ToHttp).filter(Boolean);
    for (const base of urls.reverse()) { // local mais recente primeiro
      try {
        let r;
        if (knownIdx == null) {
          const hit = await findIndex(base, nonce, msgId);
          if (!hit) continue;
          knownIdx = hit.idx; r = hit.r;
        } else {
          r = await fetch(`${base}/checkpoint_${knownIdx}_with_id.json`).then((x) => x.ok ? x.json() : null);
          if (!r) continue;
          const cid = (r.value?.message_id ?? "").toLowerCase().replace(/^0x/, "");
          if (cid !== msgId) { log(`  ⚠ validador ${validators[i].slice(0, 8)}: message_id divergente`); continue; }
        }
        const sig = (r.serialized_signature ?? "").replace(/^0x/, "");
        const root = (r.value?.checkpoint?.root ?? "").replace(/^0x/, "");
        const index = r.value?.checkpoint?.index;
        if (sig.length === 130 && root.length === 64) { found.push({ root, index, sig }); break; }
      } catch { /* tenta próximo local */ }
    }
    if (found.length >= threshold) break;
  }
  return found;
}

// ---- 4. metadata + process ----
function buildMetadata(sigs) {
  const { root, index } = sigs[0];
  const hook32 = "000000000000000000000000" + BSC.merkleHook.toLowerCase().replace(/^0x/, "");
  const idx4 = index.toString(16).padStart(8, "0");
  return hook32 + root + idx4 + sigs.map((s) => s.sig).join("");
}

const provider = new ethers.JsonRpcProvider(BSC.rpc);
const cw = await CosmWasmClient.connect(TC.rpc);
const pend = await pendingReceipts(provider, cw);
log(`recibos BSC→TC pendentes de entrega no TC: ${pend.length}`);
if (!pend.length) process.exit(0);

const { validators, threshold } = await ismRequirements(cw);
log(`ISM exige ${threshold} de ${validators.length} validadores`);

let signer = null, sender = null;
if (!DRY) {
  const hex = (process.env.TC_PRIVATE_KEY ?? "").replace(/^0x/, "");
  if (!hex) { log("⚠ falta TC_PRIVATE_KEY"); process.exit(1); }
  const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(hex, "hex")), "terra");
  sender = (await wallet.getAccounts())[0].address;
  signer = await SigningCosmWasmClient.connectWithSigner(TC.rpc, wallet, { gasPrice: GasPrice.fromString("28.325uluna") });
}

for (const p of pend) {
  log(`recibo ${p.id.slice(0, 10)}… (nonce ${p.nonce}, bloco BSC ${p.block})`);
  const sigs = await fetchSignatures(provider, validators, threshold, p.nonce, p.id);
  // só raízes iguais entram no mesmo metadata
  const byRoot = {};
  for (const s of sigs) (byRoot[s.root] ??= []).push(s);
  const best = Object.values(byRoot).sort((a, b) => b.length - a.length)[0] ?? [];
  log(`  assinaturas válidas colhidas: ${best.length}/${threshold}`);
  if (best.length < threshold) { log("  ✗ checkpoints insuficientes ainda — tento na próxima rodada"); continue; }
  const metadata = buildMetadata(best.slice(0, threshold));
  if (DRY) { log("  DRY — entregaria com metadata de", metadata.length / 2, "bytes"); continue; }
  const VAULT = "terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q";
  try {
    const res = await signer.execute(sender, TC.mailbox, { process: { metadata, message: p.msg } }, "auto");
    log(`  ✓ ENTREGUE + comissão paga — tx ${res.transactionHash} (height ${res.height})`);
  } catch (e) {
    if (String(e).includes("insufficient funds")) {
      // pool sem saldo: as tarifas ficam no IGP até alguém varrer — varre e re-tenta
      log("  pool insuficiente → Sweep{} no vault (puxa as tarifas do IGP)…");
      try {
        const sw = await signer.execute(sender, VAULT, { sweep: {} }, "auto");
        log(`  ✓ sweep tx ${sw.transactionHash}`);
        const res = await signer.execute(sender, TC.mailbox, { process: { metadata, message: p.msg } }, "auto");
        log(`  ✓ ENTREGUE + comissão paga — tx ${res.transactionHash} (height ${res.height})`);
      } catch (e2) { log(`  ✗ pós-sweep: ${String(e2).slice(0, 180)}`); }
    } else { log(`  ✗ ${String(e).slice(0, 200)}`); }
  }
}
