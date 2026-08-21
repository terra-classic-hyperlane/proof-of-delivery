// monitor-web — painel de OPERAÇÃO em tempo real (servidor local + auto-refresh).
//
// Carteiras · Pools · Serviços · VALIDADORES (checkpoints) · RPCs · MENSAGENS.
// As consultas (RPC/S3/SSH) rodam no servidor; a página só lê do localhost.
//
//   uso:  node deploy/monitor-web.mjs           # http://localhost:8787
//         PORT=9000 / --no-vps
import http from "node:http";
import fs from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const exec = promisify(execFile);
const { ethers } = await import("ethers");
const { Connection, PublicKey } = await import("@solana/web3.js");

const PORT = Number(process.env.PORT ?? 8787);
const NO_VPS = process.argv.includes("--no-vps");
const VPS = process.env.VPS_HOST ?? "root@31.97.91.4";
const HELIUS = process.env.SOLANA_RPC ?? "https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42";
const TC_LCD = process.env.TC_LCD ?? "https://lcd.terra-classic.hexxagon.io";
const BSC_RPC = process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org";
const ETH_RPC = process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com";

// os NOSSOS validadores do TC (3-de-4). checkpoint_latest_index.json no S3 anunciado.
const TC_MAINNET_DOMAIN = 132556;
const VALIDATOR_ANNOUNCE = "terra1gtnmdevekgxpvzej3wfy20e2n335gm3muwj6geduxxa86j3x70cq00asmy";
// os 4 validadores do ISM 3-de-4 (só p/ rótulo; o storage vem do on-chain)
const TC_VALIDATORS = [
  { name: "igorveras", addr: "71b2b8c36a0c76b74be92eb7915e26a69b3b03eb" },
  { name: "tcv", addr: "1afd3d07abd2aaa19a9f7993f334a926e253b90c" },
  { name: "darksun", addr: "e6bb040164a0ebbcb7e2d584f066c8b57dd74383" },
  { name: "burnitall", addr: "5c374754892ebac52702475726b67f822efdfacc" },
];
const TC_THRESHOLD = 3;
// s3://bucket/region[/prefixo] → https base
function s3http(loc) {
  const m = String(loc).match(/^s3:\/\/([^/]+)\/([^/]+)(?:\/(.*))?$/);
  return m ? `https://${m[1]}.s3.${m[2]}.amazonaws.com${m[3] ? "/" + m[3] : ""}` : null;
}

const timed = async (fn) => { const t = Date.now(); try { const v = await fn(); return { v, ms: Date.now() - t }; } catch { return { v: null, ms: Date.now() - t }; } };
// toda fonte tem timeout — uma lenta NUNCA trava o resto do snapshot
const to = (p, ms = 9000, dflt = null) => Promise.race([p, new Promise((r) => setTimeout(() => r(dflt), ms))]);
const jget = (url, ms = 8000) => fetch(url, { signal: AbortSignal.timeout(ms) }).then((r) => r.ok ? r.json() : null).catch(() => null);

async function prices() {
  const g = async (s) => Number((await jget(`https://api.binance.com/api/v3/ticker/price?symbol=${s}`, 5000))?.price || 0);
  const [LUNC, BNB, SOL, ETH] = await Promise.all([g("LUNCUSDT"), g("BNBUSDT"), g("SOLUSDT"), g("ETHUSDT")]);
  return { LUNC, BNB, SOL, ETH };
}
async function tcBalance(a) { const r = await jget(`${TC_LCD}/cosmos/bank/v1beta1/balances/${a}/by_denom?denom=uluna`); return Number(r?.balance?.amount ?? 0) / 1e6; }
async function tcQuery(c, m) { const q = Buffer.from(JSON.stringify(m)).toString("base64"); return (await jget(`${TC_LCD}/cosmwasm/wasm/v1/contract/${c}/smart/${q}`))?.data; }

// ---- validadores TC: índice assinado vs tip da árvore ----
async function validators() {
  const tip = (await tcQuery("terra183lq6yqp8km3p34cxgk6k3u78uy4plqahey6rne7n9gy98delr9qyp0n2p", { merkle_hook: { count: {} } }))?.count;
  const tipIdx = tip != null ? tip - 1 : null;
  // storage locations ANUNCIADAS na MAINNET (fonte da verdade) p/ os 4 validadores
  const ann = await tcQuery(VALIDATOR_ANNOUNCE, { get_announce_storage_locations: { validators: TC_VALIDATORS.map((v) => v.addr) } });
  const locByAddr = Object.fromEntries((ann?.storage_locations ?? []).map(([a, locs]) => [a.toLowerCase(), locs]));
  const list = await Promise.all(TC_VALIDATORS.map(async (v) => {
    const locs = locByAddr[v.addr] ?? [];
    if (!locs.length) return { name: v.name, idx: null, st: "offline", note: "não anunciou (mainnet)" };
    // usa o local anunciado mais recente que responder
    for (const base of [...locs].reverse().map(s3http).filter(Boolean)) {
      const idx = await fetch(`${base}/checkpoint_latest_index.json`, { signal: AbortSignal.timeout(7000) }).then((r) => r.ok ? r.json() : null).catch(() => null);
      if (idx == null) continue;
      const st = tipIdx == null ? "ok" : idx >= tipIdx - 2 ? "ok" : idx >= tipIdx - 10 ? "warn" : "low";
      return { name: v.name, idx, st, note: tipIdx != null && idx < tipIdx - 2 ? `${tipIdx - idx} atrás` : "atual" };
    }
    return { name: v.name, idx: null, st: "offline", note: "anunciou mas sem resposta" };
  }));
  const online = list.filter((v) => v.st === "ok" || v.st === "warn").length;
  return { tip: tipIdx, list, online, threshold: TC_THRESHOLD, healthy: online >= TC_THRESHOLD };
}

// ---- RPCs: altura + latência + up/down ----
async function rpcs() {
  const conn = new Connection(HELIUS, "confirmed");
  const [tc, bsc, eth, sol] = await Promise.all([
    timed(async () => Number((await fetch(`${TC_LCD}/cosmos/base/tendermint/v1beta1/blocks/latest`).then((r) => r.json())).block.header.height)),
    timed(async () => await new ethers.JsonRpcProvider(BSC_RPC).getBlockNumber()),
    timed(async () => await new ethers.JsonRpcProvider(ETH_RPC).getBlockNumber()),
    timed(async () => await conn.getSlot()),
  ]);
  const mk = (name, r) => ({ name, up: r.v != null, height: r.v, ms: r.ms });
  return [mk("Terra Classic", tc), mk("BSC", bsc), mk("Ethereum", eth), mk("Solana", sol)];
}

// ---- serviços + métricas do relayer (uma SSH só, grep server-side) ----
async function vpsAll() {
  if (NO_VPS) return { skipped: true };
  try {
    const svcs = ["hyperlane-relayer", "hyperlane-validator", "oracle-agent", "claim-agent", "epoch-reporter", "deliver-receipts.timer"];
    const cmd =
      `for s in ${svcs.join(" ")}; do echo "svc $s $(systemctl is-active $s 2>/dev/null)"; done; ` +
      `echo "fds $(ls /proc/$(systemctl show hyperlane-relayer -p MainPID --value)/fd 2>/dev/null | wc -l)"; ` +
      // panics: últimas 400 linhas (rápido) em vez de varrer 30 min de journal
      `echo "panics $(journalctl -u hyperlane-relayer -n 400 --no-pager 2>/dev/null | grep -c panicked)"; ` +
      // última atividade (unix ts) dos agentes de tempo — p/ calcular a próxima
      `echo "oracle_last $(stat -c %Y /root/oracle-agent/logs/agent.log 2>/dev/null || echo 0)"; ` +
      `echo "reporter_last $(journalctl -u epoch-reporter -o short-unix --no-pager 2>/dev/null | tail -1 | awk '{print int($1)}')"; ` +
      `M=$(curl -s localhost:9091/metrics 2>/dev/null); ` +
      // filas com valor > 0: nome, status, remote, valor
      `echo "$M" | grep '^hyperlane_submitter_queue_length' | grep -vE ' 0$' | sed -E 's/.*queue_name=\"([^\"]+)\".*operation_status=\"([^\"]+)\".*remote=\"([^\"]+)\".* ([0-9]+)$/queue \\3 \\1 \\4 \\2/'; ` +
      // total processado
      `echo "processed $(echo \"$M\" | grep '^hyperlane_messages_processed_count' | awk '{s+=$NF} END{print s+0}')"; ` +
      // cursor tip por chain (merkle_tree_insertion)
      `echo "$M" | grep '^hyperlane_cursor_max_sequence' | grep 'merkle_tree_insertion' | sed -E 's/.*chain=\"([^\"]+)\".* ([0-9.e+]+)$/cursor \\1 \\2/'`;
    // multiplexação: reusa UMA conexão SSH persistente (ControlPersist) — sem
    // isso o painel abre um handshake novo a cada push (~4s) e estoura o timeout.
    const { stdout } = await exec("ssh", [
      "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
      "-o", "ControlMaster=auto", "-o", `ControlPath=/tmp/tcpod-mon-%r@%h:%p`, "-o", "ControlPersist=120",
      "-o", "ServerAliveInterval=15", VPS, cmd,
    ], { timeout: 15000 });
    const out = { services: {}, queues: [], cursors: {}, panics: 0, fds: 0, processed: 0 };
    for (const line of stdout.trim().split("\n")) {
      const p = line.split(" ");
      if (p[0] === "svc") out.services[p[1]] = p[2];
      else if (p[0] === "panics") out.panics = Number(p[1]);
      else if (p[0] === "fds") out.fds = Number(p[1]);
      else if (p[0] === "processed") out.processed = Number(p[1]);
      else if (p[0] === "queue") out.queues.push({ remote: p[1], name: p[2], n: Number(p[3]), status: p.slice(4).join(" ") });
      else if (p[0] === "cursor") out.cursors[p[1]] = Number(p[2]);
      else if (p[0] === "oracle_last") out.oracleLast = Number(p[1]) || 0;
      else if (p[0] === "reporter_last") out.reporterLast = Number(p[1]) || 0;
    }
    return out;
  } catch (e) { return { error: String(e).slice(0, 80) }; }
}

// ---- atividade por operador: última tx on-chain (liveness) ----
// EVM não dá "hora da última tx" em RPC público → rastreamos o nonce e marcamos
// a hora só quando ele MUDA (persistido em disco entre reinícios).
const NCACHE = new URL("./.op-nonce.json", import.meta.url).pathname;
let nonceCache = {}; try { nonceCache = JSON.parse(fs.readFileSync(NCACHE, "utf8")); } catch { /* vazio */ }
async function lastActivity(addr, conn, eBsc, eEth) {
  try {
    if (addr.startsWith("terra")) {
      const r = await jget(`${TC_LCD}/cosmos/tx/v1beta1/txs?query=${encodeURIComponent(`message.sender='${addr}'`)}&order_by=ORDER_BY_DESC&limit=1`, 6000);
      const h = r?.tx_responses?.[0]?.timestamp;
      return { ts: h ? Math.floor(Date.parse(h) / 1000) : null };
    }
    if (addr.startsWith("0x")) {
      const prov = addr.toLowerCase() === "0xef8181201ce6c83120035ffbcc11945e67ba00ae" ? eEth : eBsc;
      const nonce = await prov.getTransactionCount(addr);
      const c = nonceCache[addr];
      if (!c) nonceCache[addr] = { nonce, ts: null };           // 1ª vez: só registra, sem cravar hora
      else if (c.nonce !== nonce) nonceCache[addr] = { nonce, ts: Math.floor(Date.now() / 1000) }; // mudou → agora
      try { fs.writeFileSync(NCACHE, JSON.stringify(nonceCache)); } catch { /* ok */ }
      return { ts: nonceCache[addr].ts, nonce }; // ts = quando o nonce mudou (ou null se ainda não vimos)
    }
    const sigs = await conn.getSignaturesForAddress(new PublicKey(addr), { limit: 1 });
    return { ts: sigs?.[0]?.blockTime ?? null };
  } catch { return { ts: null }; }
}

// ---- operadores registrados nos contratos (oráculo de preço + vault) ----
async function operators() {
  const conn = new Connection(HELIUS, "confirmed");
  const eBsc = new ethers.JsonRpcProvider(BSC_RPC), eEth = new ethers.JsonRpcProvider(ETH_RPC);
  const GA = ["function quorum() view returns (uint256)", "function operatorCount() view returns (uint256)", "function isOperator(address) view returns (bool)"];
  const parseSolOps = (d) => { // Config: pula bump+quorum+reward+edur+paused → operators(vec) — layout do rrv
    let o = 0; o += 1; const q = d[o]; o += 1 + 8 + 8 + 1; const n = d.readUInt32LE(o); o += 4;
    const ops = []; for (let i = 0; i < n; i++) { ops.push(new PublicKey(d.subarray(o, o + 32)).toBase58()); o += 32; } return { ops, q };
  };
  const parseGovOps = (d) => { let o = 1 + 32; const n = d.readUInt32LE(o); o += 4; const ops = []; for (let i = 0; i < n; i++) { ops.push(new PublicKey(d.subarray(o, o + 32)).toBase58()); o += 32; } return { ops, q: d[o] }; };
  const short = (a) => a.length > 12 ? a.slice(0, 6) + "…" + a.slice(-4) : a;
  const [tcOps, tcCfg, bscQ, bscN, bscIs, ethQ, ethN, ethIs, rrvAcc, govAcc] = await Promise.all([
    tcQuery("terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj", { operators: {} }),
    tcQuery("terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj", { config: {} }),
    eBsc && new ethers.Contract("0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5", GA, eBsc).quorum().then(Number).catch(() => null),
    new ethers.Contract("0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5", GA, eBsc).operatorCount().then(Number).catch(() => null),
    new ethers.Contract("0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5", GA, eBsc).isOperator("0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291").catch(() => false),
    new ethers.Contract("0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA", GA, eEth).quorum().then(Number).catch(() => null),
    new ethers.Contract("0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA", GA, eEth).operatorCount().then(Number).catch(() => null),
    new ethers.Contract("0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA", GA, eEth).isOperator("0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae").catch(() => false),
    conn.getAccountInfo(new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w")).catch(() => null),
    conn.getAccountInfo(PublicKey.findProgramAddressSync([Buffer.from("gov"), Buffer.from("-"), Buffer.from("config")], new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj"))[0]).catch(() => null),
  ]);
  const rrv = rrvAcc ? parseSolOps(rrvAcc.data) : { ops: [], q: null };
  const gov = govAcc ? parseGovOps(govAcc.data) : { ops: [], q: null };
  // grupos com endereços COMPLETOS
  const groups = [
    { label: "Oráculo TC", addrs: tcOps?.operators ?? [], q: tcCfg?.quorum },
    { label: "Oráculo BSC", addrs: bscIs ? ["0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291"] : [], q: bscQ, n: bscN },
    { label: "Oráculo ETH", addrs: ethIs ? ["0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae"] : [], q: ethQ, n: ethN },
    { label: "Oráculo Solana (gov)", addrs: gov.ops, q: gov.q },
    { label: "Vault Solana (rrv)", addrs: rrv.ops, q: rrv.q },
  ];
  // atividade dos endereços ÚNICOS (uma consulta por endereço)
  const uniq = [...new Set(groups.flatMap((g) => g.addrs))];
  const act = {};
  await Promise.all(uniq.map(async (a) => { act[a] = await lastActivity(a, conn, eBsc, eEth); }));
  const now = Math.floor(Date.now() / 1000);
  return groups.map((g) => ({
    label: g.label, q: g.q, n: g.n ?? g.addrs.length,
    healthy: g.q != null && (g.n ?? g.addrs.length) >= g.q,
    ops: g.addrs.map((a) => {
      const r = act[a] ?? {}; const ts = r.ts; const ageH = ts ? (now - ts) / 3600 : null;
      // ativo: atividade < 26h (oráculo 4h / época 6h dão folga). EVM sem hora
      // ainda (só nonce, RPC respondeu) = neutro "ok" até observar 1ª mudança.
      const st = ageH != null ? (ageH < 26 ? "ok" : ageH < 72 ? "warn" : "low") : (r.nonce != null ? "ok" : "warn");
      return { who: short(a), st, ageH: ageH == null ? null : Math.round(ageH * 10) / 10, nonce: r.nonce };
    }),
  }));
}

async function snapshot() {
  const P = await to(prices(), 6000, { LUNC: 0, BNB: 0, SOL: 0, ETH: 0 });
  const conn = new Connection(HELIUS, "confirmed");
  const eBsc = new ethers.JsonRpcProvider(BSC_RPC), eEth = new ethers.JsonRpcProvider(ETH_RPC);
  const [tcOp, vaultPool, tcIgp, bscOp, bscVault, ethOp, pbeo, birx, poolInfo, vals, rpcHealth, vps] = await Promise.all([
    to(tcBalance("terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp")),
    to(tcQuery("terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q", { solvency: {} })),
    to(tcBalance("terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz")),
    to(eBsc.getBalance("0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291").then((b) => Number(b) / 1e18).catch(() => null)),
    to(eBsc.getBalance("0x34E06a7793877EC5251b1dC230aD7cD577d231f4").then((b) => Number(b) / 1e18).catch(() => null)),
    to(eEth.getBalance("0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae").then((b) => Number(b) / 1e18).catch(() => null)),
    to(conn.getBalance(new PublicKey("PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS")).then((b) => b / 1e9).catch(() => null)),
    to(conn.getBalance(new PublicKey("BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j")).then((b) => b / 1e9).catch(() => null)),
    to(conn.getAccountInfo(new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w")).catch(() => null)),
    to(validators(), 12000, { tip: null, list: [], online: 0, threshold: TC_THRESHOLD, healthy: false }),
    to(rpcs(), 10000, []),
    Promise.resolve(vpsCache), // VPS vem do loop próprio (cache) — NÃO bloqueia o snapshot
  ]);
  const ops = await to(operators(), 10000, []);
  // preços que o oracle-agent escreveu on-chain (o que o usuário do TC paga)
  const ORACLE = "terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d";
  const oraclePrices = await Promise.all([[1, "→ETH"], [56, "→BSC"], [1399811149, "→SOL"]].map(async ([dom, lbl]) => {
    const r = await tcQuery(ORACLE, { oracle: { get_exchange_rate_and_gas_price: { dest_domain: dom } } });
    return { lbl, rate: r?.exchange_rate, gas: r?.gas_price };
  }));
  let sol = null;
  if (poolInfo) {
    const d = poolInfo.data; let o = 0; const u8 = () => d[o++]; const r8 = () => { let v = 0n; for (let i = 0; i < 8; i++) v |= BigInt(d[o + i]) << BigInt(8 * i); o += 8; return v; };
    u8(); u8(); r8(); r8(); u8(); const n = d.readUInt32LE(o); o += 4 + n * 32; const tc = r8(); const base = Number(r8());
    sol = { pool: poolInfo.lamports / 1e9, credited: Number(tc) / 1e9, base, nowEpoch: Math.floor(Date.now() / 1000 / 21600) };
  }
  const st = (v, lo) => v == null ? "err" : v < lo ? "low" : "ok";
  return {
    ts: new Date().toISOString(), prices: P,
    wallets: [
      { id: "TC operador (terra1run9wz)", v: tcOp, sym: "LUNC", usd: tcOp * P.LUNC, st: st(tcOp, 200) },
      { id: "BSC gatilho (0x8f08)", v: bscOp, sym: "BNB", usd: (bscOp ?? 0) * P.BNB, st: st(bscOp, 0.01) },
      { id: "ETH operador (0xEF81)", v: ethOp, sym: "ETH", usd: (ethOp ?? 0) * P.ETH, st: st(ethOp, 0.005) },
      { id: "SOL PbEo (reporter)", v: pbeo, sym: "SOL", usd: (pbeo ?? 0) * P.SOL, st: st(pbeo, 0.02) },
      { id: "SOL BirXd4Q (reserva)", v: birx, sym: "SOL", usd: (birx ?? 0) * P.SOL, st: st(birx, 0.1) },
    ],
    pools: [
      { id: "TC vault pool", v: vaultPool ? Number(vaultPool.pool.amount) / 1e6 : null, sym: "LUNC", note: vaultPool ? `cobre ${Math.floor((Number(vaultPool.pool.amount) / 1e6) / 1584)} comissões` : "sem resposta" },
      { id: "TC IGP acumulado", v: tcIgp, sym: "LUNC", note: tcIgp > 1584 ? "varrer (Sweep)" : "" },
      { id: "BSC vault", v: bscVault, sym: "BNB", note: "" },
      { id: "SOL pod pool", v: sol?.pool, sym: "SOL", note: sol ? `replay base ${sol.base}/${sol.nowEpoch} ${sol.base > 0 && sol.nowEpoch - sol.base < 512 ? "ok" : "checar"}` : "" },
    ],
    validators: vals, rpcs: rpcHealth, vps, operators: ops,
    timing: {
      nowEpoch: Math.floor(Date.now() / 1000 / 21600),
      epochClosesAt: (Math.floor(Date.now() / 1000 / 21600) + 1) * 21600, // unix ts do fim da época atual
      oracleIntervalS: 14400, reporterLoopS: 3600,
      oracleLast: vps?.oracleLast ?? null, reporterLast: vps?.reporterLast ?? null,
      oraclePrices,
    },
  };
}

const HTML = `<!doctype html><html lang="pt"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>tc-pod · operação</title><style>
:root{--bg:#0d1117;--card:#161b22;--bd:#30363d;--tx:#e6edf3;--dim:#8b949e;--ok:#3fb950;--low:#f85149;--warn:#d29922;--cy:#58a6ff}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--tx);font:13px/1.5 ui-monospace,Menlo,monospace}
header{padding:14px 20px;border-bottom:1px solid var(--bd);display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px}
h1{font-size:15px;margin:0;color:var(--cy)}.px{color:var(--dim);font-size:12px}#pulse{display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--ok);margin-right:6px;transition:opacity .2s}
main{padding:18px;display:grid;gap:16px;grid-template-columns:repeat(auto-fit,minmax(330px,1fr));max-width:1400px}
.card{background:var(--card);border:1px solid var(--bd);border-radius:8px;overflow:hidden}
.card h2{font-size:12px;margin:0;padding:9px 14px;border-bottom:1px solid var(--bd);color:var(--cy);background:#1c2128;display:flex;justify-content:space-between}
.rowi{display:flex;justify-content:space-between;padding:7px 14px;border-bottom:1px solid #21262d;gap:10px}.rowi:last-child{border:0}
.lbl{color:var(--dim)}.val{text-align:right}.u{color:var(--dim);font-size:11px;margin-left:5px}
.badge{padding:1px 7px;border-radius:10px;font-size:11px;font-weight:600}
.ok{background:rgba(63,185,80,.15);color:var(--ok)}.low,.err,.offline{background:rgba(248,81,73,.15);color:var(--low)}
.warn{background:rgba(210,153,34,.15);color:var(--warn)}.note{color:var(--dim);font-size:11px}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px}#err{color:var(--low);padding:0 20px}
</style></head><body>
<header><h1>▓ tc-proof-of-delivery · operação</h1><div class="px"><span id="pulse"></span><span id="px">conectando…</span> <span id="age" class="note"></span></div></header>
<div id="err"></div><main id="main"></main>
<script>
const money=(v,s)=>v==null?'<span class="badge err">err</span>':Number(v).toFixed(s==='LUNC'?2:s==='SOL'?4:6)+' '+s;
const row=(l,v,x='')=>\`<div class="rowi"><span class="lbl">\${l}</span><span class="val">\${v} \${x}</span></div>\`;
const card=(t,rows,sub='')=>\`<div class="card"><h2><span>\${t}</span><span class="note">\${sub}</span></h2>\${rows.join('')}</div>\`;
const bdg=(s,txt)=>'<span class="badge '+s+'">'+(txt||s)+'</span>';
function render(d){
    const pulse=document.getElementById('pulse'); pulse.style.opacity=.3; setTimeout(()=>pulse.style.opacity=1,200);
    const P=d.prices;
    document.getElementById('px').textContent=\`LUNC $\${P.LUNC.toFixed(8)} · BNB $\${P.BNB.toFixed(2)} · SOL $\${P.SOL.toFixed(2)} · ETH $\${P.ETH.toFixed(2)} · \${d.ts.slice(0,19).replace('T',' ')} UTC\`;
    document.getElementById('err').textContent='';
    const cards=[];
    // Validadores TC
    const V=d.validators;
    cards.push(card('Validadores TC (checkpoints)',
      V.list.map(v=>row(v.name, v.idx==null?'—':('idx '+v.idx), bdg(v.st==='ok'?'ok':v.st, v.st==='offline'?'OFFLINE':v.st==='low'?'ATRASADO':v.note))),
      'tip '+(V.tip??'?')+' · '+bdg(V.healthy?'ok':'low', V.online+'/'+V.list.length+' (min '+V.threshold+')')));
    // Operadores (por contrato) — com atividade individual
    if(d.operators&&d.operators.length){
      const rows=[];
      const fmtAge=(h)=>h==null?'—':h<1?Math.round(h*60)+'m':h<48?h.toFixed(1)+'h':Math.round(h/24)+'d';
      d.operators.forEach(g=>{
        rows.push(row('<b>'+g.label+'</b>', '', bdg(g.healthy?'ok':'low', (g.q!=null?g.q+'-de-'+g.n:'?'))));
        if(!g.ops.length) rows.push(row('&nbsp;&nbsp;—','','<span class="note">nenhum</span>'));
        g.ops.forEach(o=>rows.push(row('&nbsp;&nbsp;'+o.who,
          bdg(o.st, o.st==='ok'?'ativo':o.st==='warn'?'lento':'inativo'),
          '<span class="note">'+(o.ageH!=null?'há '+fmtAge(o.ageH):(o.nonce!=null?'nonce '+o.nonce:'sem tx'))+'</span>')));
      });
      cards.push(card('Operadores (atividade)', rows));
    }
    // RPCs
    cards.push(card('RPCs (saúde + latência)',
      d.rpcs.map(r=>row(r.name, r.up?('bloco '+r.height):bdg('low','DOWN'), r.up?'<span class="note">'+r.ms+'ms</span>':''))));
    // Mensagens (relayer)
    const vp=d.vps;
    let msgs;
    if(vp.skipped) msgs=[row('(--no-vps)','')];
    else if(vp.error) msgs=[row('SSH',bdg('err','inacessível'),vp.error)];
    else{
      msgs=[];
      msgs.push(row('processadas (total)', vp.processed??0));
      const q=vp.queues||[];
      if(!q.length) msgs.push(row('em trânsito/presas', bdg('ok','0 — fila limpa')));
      else q.forEach(x=>msgs.push(row('→ '+x.remote+' ('+x.name.replace('_queue','')+')', bdg('warn',x.n), '<span class="note">'+x.status+'</span>')));
      Object.entries(vp.cursors||{}).forEach(([c,n])=>msgs.push(row('tip '+c, n)));
    }
    cards.push(card('Mensagens (relayer)', msgs));
    // Épocas & Oracle
    const T=d.timing, nowS=Date.now()/1000;
    const fmtIn=(ts)=>{ if(!ts) return '—'; const s=Math.round(ts-nowS); if(s<=0) return 'agora'; const h=Math.floor(s/3600),m=Math.floor(s%3600/60); return h>0?h+'h'+m+'m':m+'m'; };
    const fmtAgo=(ts)=>{ if(!ts) return '—'; const s=Math.round(nowS-ts); const h=Math.floor(s/3600),m=Math.floor(s%3600/60); return (h>0?h+'h'+m+'m':m+'m')+' atrás'; };
    const or=[];
    or.push(row('época atual (TC→Solana)', T.nowEpoch, '<span class="note">fecha em '+fmtIn(T.epochClosesAt)+'</span>'));
    or.push(row('epoch-reporter última', fmtAgo(T.reporterLast), '<span class="note">loop '+(T.reporterLoopS/3600)+'h · próxima em '+fmtIn((T.reporterLast||nowS)+T.reporterLoopS)+'</span>'));
    or.push(row('oracle-agent última', fmtAgo(T.oracleLast), '<span class="note">a cada '+(T.oracleIntervalS/3600)+'h · próxima em '+fmtIn((T.oracleLast||nowS)+T.oracleIntervalS)+'</span>'));
    (T.oraclePrices||[]).forEach(p=>or.push(row('preço '+p.lbl, p.rate?('rate '+p.rate):'—', p.gas?'<span class="note">gas '+p.gas+'</span>':'')));
    cards.push(card('Épocas & Oracle', or));
    // Carteiras
    cards.push(card('Carteiras-gatilho (gás)', d.wallets.map(w=>row(w.id, money(w.v,w.sym)+' <span class="u">($'+w.usd.toFixed(2)+')</span>', bdg(w.st, w.st==='low'?'BAIXO':w.st)))));
    // Pools
    cards.push(card('Pools (comissões)', d.pools.map(p=>row(p.id, money(p.v,p.sym), p.note?'<span class="note">'+p.note+'</span>':''))));
    // Serviços
    let sv;
    if(vp.skipped) sv=[row('(--no-vps)','')];
    else if(vp.error) sv=[row('SSH',bdg('err','inacessível'),vp.error)];
    else{ sv=['hyperlane-relayer','hyperlane-validator','oracle-agent','claim-agent','epoch-reporter','deliver-receipts.timer'].map(s=>{
        const a=vp.services[s]==='active';return row(s,'<span class="dot" style="background:'+(a?'var(--ok)':'var(--low)')+'"></span>'+(vp.services[s]||'?'));});
      sv.push(row('relayer panics (30m)', bdg(vp.panics>0?'low':'ok', vp.panics), '<span class="note">fds '+vp.fds+'</span>'));
    }
    cards.push(card('Serviços (VPS)', sv));
    document.getElementById('main').innerHTML=cards.join('');
}
// SSE: o servidor empurra um snapshot novo assim que fica pronto (~4s)
let lastAt=0;
function connect(){
  const es=new EventSource('/stream');
  es.onmessage=(ev)=>{ try{ render(JSON.parse(ev.data)); lastAt=Date.now(); document.getElementById('err').textContent=''; }catch(e){} };
  es.onerror=()=>{ document.getElementById('err').textContent='reconectando…'; es.close(); setTimeout(connect,3000); };
}
connect();
// contador "há Xs" ao vivo (prova visual de que está fluindo)
setInterval(()=>{ const a=document.getElementById('age'); if(!lastAt){a.textContent='';return;}
  const s=Math.round((Date.now()-lastAt)/1000); a.textContent='· atualizado há '+s+'s'; a.style.color=s>15?'var(--low)':'var(--dim)';
  document.getElementById('px').textContent||(document.getElementById('px').textContent='ao vivo');
},1000);
</script></body></html>`;

// ---- VPS/SSH num loop PRÓPRIO (~15s): a parte lenta nunca trava a rápida ----
let vpsCache = NO_VPS ? { skipped: true } : { error: "iniciando…" };
async function vpsLoop() {
  for (;;) {
    if (!NO_VPS) { const r = await to(vpsAll(), 14000, null); if (r) vpsCache = r; else vpsCache = { error: "timeout" }; }
    await new Promise((r) => setTimeout(r, 15000));
  }
}
vpsLoop();

// ---- push contínuo dos dados RÁPIDOS (~3s): on-chain, empurrado via SSE ----
let latest = null, clients = new Set();
async function pump() {
  for (;;) {
    const t = Date.now();
    try { latest = await snapshot(); const line = `data: ${JSON.stringify(latest)}\n\n`; for (const c of clients) c.write(line); } catch { /* segue */ }
    await new Promise((r) => setTimeout(r, Math.max(0, 3000 - (Date.now() - t)))); // ~3s de piso
  }
}
pump();

http.createServer(async (req, res) => {
  if (req.url === "/stream") {
    res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
    clients.add(res);
    if (latest) res.write(`data: ${JSON.stringify(latest)}\n\n`);
    const ka = setInterval(() => res.write(": ka\n\n"), 15000); // keep-alive
    req.on("close", () => { clearInterval(ka); clients.delete(res); });
  } else if (req.url === "/api") {
    try { res.writeHead(200, { "content-type": "application/json" }); res.end(JSON.stringify(latest ?? await snapshot())); }
    catch (e) { res.writeHead(500); res.end(JSON.stringify({ error: String(e) })); }
  } else { res.writeHead(200, { "content-type": "text/html; charset=utf-8" }); res.end(HTML); }
}).listen(PORT, () => console.log(`\n  painel em → http://localhost:${PORT}  (tempo real via SSE)\n`));
