// monitor — painel de saúde do tc-proof-of-delivery (4 chains + VPS num lugar só).
//
//   uso:  node deploy/monitor.mjs            # painel completo (RPC + SSH na VPS)
//         node deploy/monitor.mjs --no-vps   # pula o SSH (só on-chain)
//         node deploy/monitor.mjs --watch    # atualiza a cada 60s
//   env:  SOLANA_RPC (default Helius), TC_LCD, BSC_RPC, ETH_RPC, VPS_HOST
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const exec = promisify(execFile);
const { ethers } = await import("ethers");
const { Connection, PublicKey } = await import("@solana/web3.js");

const NO_VPS = process.argv.includes("--no-vps");
const WATCH = process.argv.includes("--watch");
const VPS = process.env.VPS_HOST ?? "root@31.97.91.4";
const HELIUS = process.env.SOLANA_RPC ?? "https://mainnet.helius-rpc.com/?api-key=cc0650d4-3439-4adf-9ac1-01ea008e7a42";
const TC_LCD = process.env.TC_LCD ?? "https://lcd.terra-classic.hexxagon.io";
const BSC_RPC = process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org";
const ETH_RPC = process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com";

// cores
const C = { r: "\x1b[0m", b: "\x1b[1m", dim: "\x1b[2m", grn: "\x1b[32m", red: "\x1b[31m", yel: "\x1b[33m", cyan: "\x1b[36m", mag: "\x1b[35m" };
const ok = (s) => `${C.grn}${s}${C.r}`, bad = (s) => `${C.red}${s}${C.r}`, warn = (s) => `${C.yel}${s}${C.r}`;
const head = (s) => console.log(`\n${C.b}${C.cyan}▓▓ ${s}${C.r}`);
const row = (label, val, note = "") => console.log(`  ${label.padEnd(26)} ${val}${note ? "  " + C.dim + note + C.r : ""}`);

// preços ao vivo (Binance) p/ converter saldos em $
async function prices() {
  const g = async (s) => Number((await fetch(`https://api.binance.com/api/v3/ticker/price?symbol=${s}`).then((r) => r.json())).price || 0);
  const [LUNC, BNB, SOL, ETH] = await Promise.all([g("LUNCUSDT"), g("BNBUSDT"), g("SOLUSDT"), g("ETHUSDT")]);
  return { LUNC, BNB, SOL, ETH };
}
const usd = (n, p) => `${C.dim}($${(n * p).toFixed(2)})${C.r}`;
// alerta de saldo baixo por chain (em unidades da moeda)
const flag = (v, lo) => (v < lo ? bad("⚠ BAIXO") : ok("ok"));

async function tcBalance(addr, denom = "uluna") {
  const r = await fetch(`${TC_LCD}/cosmos/bank/v1beta1/balances/${addr}/by_denom?denom=${denom}`).then((x) => x.json()).catch(() => null);
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
  } catch (e) { return { error: String(e).slice(0, 60) }; }
}

async function snapshot() {
  const P = await prices();
  const conn = new Connection(HELIUS, "confirmed");
  const eBsc = new ethers.JsonRpcProvider(BSC_RPC), eEth = new ethers.JsonRpcProvider(ETH_RPC);

  // dispara tudo em paralelo
  const [
    tcOp, tcVaultPool, tcIgpPool,
    bscOp, bscVault,
    ethOp,
    pbeo, birx, poolInfo,
    epochBase, vps,
  ] = await Promise.all([
    tcBalance("terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp"),
    tcQuery("terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q", { solvency: {} }),
    tcBalance("terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz"),
    eBsc.getBalance("0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291").then((b) => Number(b) / 1e18).catch(() => 0),
    eBsc.getBalance("0x34E06a7793877EC5251b1dC230aD7cD577d231f4").then((b) => Number(b) / 1e18).catch(() => 0),
    eEth.getBalance("0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae").then((b) => Number(b) / 1e18).catch(() => 0),
    conn.getBalance(new PublicKey("PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS")).then((b) => b / 1e9).catch(() => 0),
    conn.getBalance(new PublicKey("BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j")).then((b) => b / 1e9).catch(() => 0),
    conn.getAccountInfo(new PublicKey("Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w")).catch(() => null),
    Promise.resolve(null), // placeholder
    vpsServices(),
  ]);

  console.clear();
  console.log(`${C.b}${C.mag}╔════ tc-proof-of-delivery · painel · ${new Date().toISOString().slice(0, 19).replace("T", " ")} UTC ════╗${C.r}`);
  console.log(`  ${C.dim}LUNC $${P.LUNC.toFixed(8)} · BNB $${P.BNB.toFixed(2)} · SOL $${P.SOL.toFixed(2)} · ETH $${P.ETH.toFixed(2)}${C.r}`);

  head("Carteiras-gatilho (pagam o gás — precisam de saldo)");
  row("TC operador (terra1run9wz)", `${tcOp.toFixed(2)} LUNC ${usd(tcOp, P.LUNC)}`, flag(tcOp, 200));
  row("BSC gatilho (0x8f08)", `${bscOp.toFixed(5)} BNB ${usd(bscOp, P.BNB)}`, flag(bscOp, 0.01));
  row("ETH operador (0xEF81)", `${ethOp.toFixed(6)} ETH ${usd(ethOp, P.ETH)}`, flag(ethOp, 0.005));
  row("SOL PbEo (reporter)", `${pbeo.toFixed(4)} SOL ${usd(pbeo, P.SOL)}`, flag(pbeo, 0.02));
  row("SOL BirXd4Q (authority)", `${birx.toFixed(4)} SOL ${usd(birx, P.SOL)}`, flag(birx, 0.1));

  head("Pools (reserva das comissões)");
  const vp = tcVaultPool ? Number(tcVaultPool.pool.amount) / 1e6 : 0;
  row("TC vault pool", `${vp.toFixed(2)} LUNC ${usd(vp, P.LUNC)}`, tcVaultPool ? `cobre ${Math.floor(vp / 1584)} comissões` : bad("sem resposta"));
  row("TC IGP acumulado", `${tcIgpPool.toFixed(2)} LUNC ${usd(tcIgpPool, P.LUNC)}`, tcIgpPool > 1584 ? warn("varrer p/ o pool (Sweep)") : "");
  row("BSC vault", `${bscVault.toFixed(5)} BNB ${usd(bscVault, P.BNB)}`);
  if (poolInfo) {
    // Config: … total_credited(u64) applied_base(u64) na cauda
    const d = poolInfo.data; let o = 0; const u8 = () => d[o++]; const r8 = () => { let v = 0n; for (let i = 0; i < 8; i++) v |= BigInt(d[o + i]) << BigInt(8 * i); o += 8; return v; };
    u8(); u8(); r8(); r8(); u8(); const n = d.readUInt32LE(o); o += 4 + n * 32; const tc = r8(); const base = Number(r8());
    row("SOL pod pool (Config)", `${(poolInfo.lamports / 1e9).toFixed(4)} SOL ${usd(poolInfo.lamports / 1e9, P.SOL)}`, `total creditado ${(Number(tc) / 1e9).toFixed(4)} SOL`);
    const nowEpoch = Math.floor(Date.now() / 1000 / 21600);
    row("SOL replay base / época", `${base} / ${nowEpoch}`, base > 0 && nowEpoch - base < 512 ? ok("janela ok") : warn("checar SetAppliedBase"));
  }

  head("Serviços (VPS 31.97.91.4)");
  if (NO_VPS) console.log(`  ${C.dim}(--no-vps: pulado)${C.r}`);
  else if (!vps || vps.error) row("SSH", bad("inacessível"), vps?.error ?? "");
  else {
    for (const s of ["hyperlane-relayer", "hyperlane-validator", "oracle-agent", "claim-agent", "epoch-reporter", "deliver-receipts.timer"]) {
      row(s, vps[s] === "active" ? ok("active") : bad(vps[s] ?? "?"));
    }
    row("relayer panics (30min)", Number(vps.relayer_panics) > 0 ? bad(vps.relayer_panics) : ok("0"),
      `fds ${vps.relayer_fds ?? "?"}`);
  }
  console.log(`\n${C.dim}  (⚠ BAIXO = recarregar a carteira · IGP acumulado alto = rodar Sweep no vault)${C.r}`);
}

if (WATCH) {
  const loop = async () => { try { await snapshot(); } catch (e) { console.error("erro:", String(e).slice(0, 120)); } };
  await loop(); setInterval(loop, 60_000);
} else { await snapshot(); }
