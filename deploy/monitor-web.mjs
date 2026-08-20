// monitor-web — painel de saúde em PÁGINA WEB (servidor local).
//
// Roda no teu PC: consulta RPC/SSH do lado do servidor e serve uma página que
// atualiza sozinha. Abra no navegador — nada vai pra internet (é localhost).
//
//   uso:  node deploy/monitor-web.mjs           # http://localhost:8787
//         PORT=9000 node deploy/monitor-web.mjs
//         node deploy/monitor-web.mjs --no-vps  # sem SSH na VPS
import http from "node:http";
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

async function prices() {
  const g = async (s) => Number((await fetch(`https://api.binance.com/api/v3/ticker/price?symbol=${s}`).then((r) => r.json())).price || 0);
  const [LUNC, BNB, SOL, ETH] = await Promise.all([g("LUNCUSDT"), g("BNBUSDT"), g("SOLUSDT"), g("ETHUSDT")]);
  return { LUNC, BNB, SOL, ETH };
}
async function tcBalance(addr) {
  const r = await fetch(`${TC_LCD}/cosmos/bank/v1beta1/balances/${addr}/by_denom?denom=uluna`).then((x) => x.json()).catch(() => null);
  return Number(r?.balance?.amount ?? 0) / 1e6;
}
async function tcQuery(contract, msg) {
  const q = Buffer.from(JSON.stringify(msg)).toString("base64");
  return (await fetch(`${TC_LCD}/cosmwasm/wasm/v1/contract/${contract}/smart/${q}`).then((x) => x.json()).catch(() => null))?.data;
}
async function vpsServices() {
  if (NO_VPS) return null;
  try {
    const svcs = ["hyperlane-relayer", "hyperlane-validator", "oracle-agent", "claim-agent", "epoch-reporter", "deliver-receipts.timer"];
    const { stdout } = await exec("ssh", ["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", VPS,
      `for s in ${svcs.join(" ")}; do echo "$s=$(systemctl is-active $s 2>/dev/null)"; done; ` +
      `echo "relayer_panics=$(journalctl -u hyperlane-relayer --since '-30 min' --no-pager 2>/dev/null | grep -c panicked)"; ` +
      `echo "relayer_fds=$(ls /proc/$(systemctl show hyperlane-relayer -p MainPID --value)/fd 2>/dev/null | wc -l)"`]);
    return Object.fromEntries(stdout.trim().split("\n").map((l) => l.split("=")));
  } catch (e) { return { error: String(e).slice(0, 80) }; }
}

async function snapshot() {
  const P = await prices();
  const conn = new Connection(HELIUS, "confirmed");
  const eBsc = new ethers.JsonRpcProvider(BSC_RPC), eEth = new ethers.JsonRpcProvider(ETH_RPC);
  const [tcOp, vaultPool, tcIgp, bscOp, bscVault, ethOp, pbeo, birx, poolInfo, vps] = await Promise.all([
    tcBalance("terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp"),
    tcQuery("terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q", { solvency: {} }),
    tcBalance("terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz"),
    eBsc.getBalance("0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291").then((b) => Number(b) / 1e18).catch(() => null),
    eBsc.getBalance("0x34E06a7793877EC5251b1dC230aD7cD577d231f4").then((b) => Number(b) / 1e18).catch(() => null),
    eEth.getBalance("0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae").then((b) => Number(b) / 1e18).catch(() => null),
    conn.getBalance(new PublicKey("PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS")).then((b) => b / 1e9).catch(() => null),
    conn.getBalance(new PublicKey("BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j")).then((b) => b / 1e9).catch(() => null),
    conn.getAccountInfo(new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w")).catch(() => null),
    vpsServices(),
  ]);
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
      { id: "SOL BirXd4Q (reserva/authority)", v: birx, sym: "SOL", usd: (birx ?? 0) * P.SOL, st: st(birx, 0.1) },
    ],
    pools: [
      { id: "TC vault pool", v: vaultPool ? Number(vaultPool.pool.amount) / 1e6 : null, sym: "LUNC", note: vaultPool ? `cobre ${Math.floor((Number(vaultPool.pool.amount) / 1e6) / 1584)} comissões` : "sem resposta" },
      { id: "TC IGP acumulado", v: tcIgp, sym: "LUNC", note: tcIgp > 1584 ? "varrer (Sweep)" : "" },
      { id: "BSC vault", v: bscVault, sym: "BNB", note: "" },
      { id: "SOL pod pool", v: sol?.pool, sym: "SOL", note: sol ? `creditado ${sol.credited.toFixed(4)} SOL · replay base ${sol.base}/${sol.nowEpoch} ${sol.base > 0 && sol.nowEpoch - sol.base < 512 ? "ok" : "checar"}` : "" },
    ],
    services: NO_VPS ? { skipped: true } : vps,
  };
}

const HTML = `<!doctype html><html lang="pt"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>tc-proof-of-delivery · painel</title><style>
:root{--bg:#0d1117;--card:#161b22;--bd:#30363d;--tx:#e6edf3;--dim:#8b949e;--ok:#3fb950;--low:#f85149;--warn:#d29922;--cy:#58a6ff}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--tx);font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}
header{padding:16px 20px;border-bottom:1px solid var(--bd);display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px}
h1{font-size:16px;margin:0;color:var(--cy)}.px{color:var(--dim);font-size:12px}
main{padding:20px;display:grid;gap:20px;grid-template-columns:repeat(auto-fit,minmax(340px,1fr));max-width:1200px}
.card{background:var(--card);border:1px solid var(--bd);border-radius:8px;overflow:hidden}
.card h2{font-size:13px;margin:0;padding:10px 14px;border-bottom:1px solid var(--bd);color:var(--cy);background:#1c2128}
.rowi{display:flex;justify-content:space-between;padding:8px 14px;border-bottom:1px solid #21262d;gap:10px}
.rowi:last-child{border:0}.lbl{color:var(--dim)}.val{text-align:right}.u{color:var(--dim);font-size:12px;margin-left:6px}
.badge{padding:1px 7px;border-radius:10px;font-size:11px;font-weight:600}
.ok{background:rgba(63,185,80,.15);color:var(--ok)}.low{background:rgba(248,81,73,.15);color:var(--low)}
.warn{background:rgba(210,153,34,.15);color:var(--warn)}.err{background:rgba(248,81,73,.15);color:var(--low)}
.note{color:var(--dim);font-size:12px}#err{color:var(--low);padding:0 20px}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px}
</style></head><body>
<header><h1>▓ tc-proof-of-delivery · painel</h1><div class="px" id="px">carregando…</div></header>
<div id="err"></div><main id="main"></main>
<script>
const money=(v,s)=>v==null?'<span class="badge err">err</span>':v.toFixed(s==='LUNC'?2:s==='SOL'?4:6)+' '+s;
async function tick(){
  try{
    const d=await (await fetch('/api')).json();
    const P=d.prices;
    document.getElementById('px').textContent=
      \`LUNC $\${P.LUNC.toFixed(8)} · BNB $\${P.BNB.toFixed(2)} · SOL $\${P.SOL.toFixed(2)} · ETH $\${P.ETH.toFixed(2)}  ·  \${d.ts.slice(0,19).replace('T',' ')} UTC\`;
    document.getElementById('err').textContent='';
    const cards=[];
    // carteiras
    cards.push(card('Carteiras-gatilho (gás)', d.wallets.map(w=>
      row(w.id, money(w.v,w.sym)+' <span class="u">($'+w.usd.toFixed(2)+')</span>', '<span class="badge '+w.st+'">'+(w.st==='low'?'BAIXO':w.st)+'</span>'))));
    // pools
    cards.push(card('Pools (reserva das comissões)', d.pools.map(p=>
      row(p.id, money(p.v,p.sym), p.note?'<span class="note">'+p.note+'</span>':''))));
    // serviços
    let sv;
    if(d.services&&d.services.skipped) sv=[row('(--no-vps)','','')];
    else if(!d.services||d.services.error) sv=[row('SSH','<span class="badge err">inacessível</span>',d.services?.error||'')];
    else{ sv=['hyperlane-relayer','hyperlane-validator','oracle-agent','claim-agent','epoch-reporter','deliver-receipts.timer'].map(s=>{
      const a=d.services[s]==='active';return row(s,'<span class="dot" style="background:'+(a?'var(--ok)':'var(--low)')+'"></span>'+(d.services[s]||'?'),'');});
      const p=Number(d.services.relayer_panics||0);
      sv.push(row('relayer panics (30m)','<span class="badge '+(p>0?'low':'ok')+'">'+p+'</span>','<span class="note">fds '+(d.services.relayer_fds||'?')+'</span>'));
    }
    cards.push(card('Serviços (VPS)', sv));
    document.getElementById('main').innerHTML=cards.join('');
  }catch(e){ document.getElementById('err').textContent='erro ao consultar: '+e.message; }
}
const row=(l,v,x)=>\`<div class="rowi"><span class="lbl">\${l}</span><span class="val">\${v} \${x||''}</span></div>\`;
const card=(t,rows)=>\`<div class="card"><h2>\${t}</h2>\${rows.join('')}</div>\`;
tick(); setInterval(tick, 30000);
</script></body></html>`;

http.createServer(async (req, res) => {
  if (req.url === "/api") {
    try { const s = await snapshot(); res.writeHead(200, { "content-type": "application/json" }); res.end(JSON.stringify(s)); }
    catch (e) { res.writeHead(500); res.end(JSON.stringify({ error: String(e) })); }
  } else { res.writeHead(200, { "content-type": "text/html; charset=utf-8" }); res.end(HTML); }
}).listen(PORT, () => console.log(`\n  painel web em → http://localhost:${PORT}\n  (Ctrl+C para parar)\n`));
