// oracle-agent — o lado off-chain do operador (spec §03/§10, generalizado):
// a cada intervalo, observa preço (CoinGecko) e gás dos domínios remotos e
// submete SubmitPrice ao GOVERNOR de cada chain local habilitada.
//
// O agente NÃO decide nada sozinho: quórum, mediana, faixa e delta são
// aplicados on-chain pelos governors. Um erro numa chain não derruba as outras.
//
// Uso:  node src/index.js [--once] [--dry-run] [--config caminho.json]
//   --dry-run: calcula e imprime o que submeteria, sem assinar nada
//   --once:    uma rodada só (útil em cron); sem ele, loop com intervalo

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fetchUsdPrices, exchangeRate, fetchRemoteGasPrice } from "./prices.js";
import { runClaims } from "./claims.js";

const argv = process.argv.slice(2);
const DRY = argv.includes("--dry-run");
const ONCE = argv.includes("--once");
const cfgArg = argv.indexOf("--config");
const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const cfgPath = cfgArg >= 0 ? argv[cfgArg + 1] : path.join(root, "config.json");
const examplePath = path.join(root, "config.example.json");

const config = JSON.parse(
  fs.readFileSync(fs.existsSync(cfgPath) ? cfgPath : examplePath, "utf8"),
);
if (!fs.existsSync(cfgPath)) {
  console.warn(`[agent] config.json não encontrado — usando ${path.basename(examplePath)} (só faz sentido com --dry-run)`);
}

const log = (chain, msg) => console.log(`[${new Date().toISOString()}] [${chain}] ${msg}`);

// ---------------------------------------------------------------------------
// MODO ÂNCORA (produção é a verdade): em vez de calcular o rate do zero (cada
// deployment tem calibração própria!), o agente ancora no valor ON-CHAIN
// vigente na primeira rodada e, depois, só o AJUSTA pela variação RELATIVA do
// preço (rate) e do gás observado (gas). Quórum/faixa/delta seguem on-chain.
// Recalibrou manualmente o oracle? Apague a entrada no state.json → re-ancora.
// ---------------------------------------------------------------------------
const statePath = config.statePath ?? path.join(root, "state.json");
const state = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, "utf8")) : {};
const saveState = () => fs.writeFileSync(statePath, JSON.stringify(state, null, 2));
const MIN_CHANGE_BPS = config.minChangeBps ?? 300; // só submete se drift >= 3%

const mods = {
  cosmwasm: () => import("./chains/terraclassic.js"),
  evm: () => import("./chains/evm.js"),
  solana: () => import("./chains/solana.js"),
};

async function makeSubmitter(name, chain) {
  const mod = await mods[chain.type]?.();
  if (!mod) throw new Error(`[${name}] type desconhecido: ${chain.type}`);
  switch (chain.type) {
    case "cosmwasm": return mod.makeCosmwasmSubmitter(chain);
    case "evm": return mod.makeEvmSubmitter(chain);
    case "solana": return mod.makeSolanaSubmitter(chain, config.epochDurationSecs ?? 21_600);
  }
}

async function runChain(name, chain, usd) {
  const mod = await mods[chain.type]?.();
  if (!mod) throw new Error(`[${name}] type desconhecido: ${chain.type}`);
  let submitter = null; // criado só quando há algo a submeter (dry-run não exige chave)
  for (const [domain, remote] of Object.entries(chain.remotes)) {
    try {
      const cur = await mod.readOracle(chain, domain); // VIGENTE on-chain
      const ratioNow = usd[remote.coin] / usd[chain.localCoin];
      const gasObs = BigInt(await fetchRemoteGasPrice(remote.gasPriceSource));
      const key = `${name}:${domain}`;
      const anchor = state[key];

      if (!anchor) {
        log(name, `domínio ${domain} (${remote.coin}): âncora ${DRY ? "seria criada" : "criada"} no vigente rate=${cur.rate} gas=${cur.gas} (ratio=${ratioNow.toExponential(4)}) — nada submetido`);
        if (!DRY) {
          state[key] = { rate: cur.rate.toString(), ratio: ratioNow, gas: cur.gas.toString(), gasObs: gasObs.toString(), ts: new Date().toISOString() };
          saveState();
        }
        continue;
      }

      const candRate = BigInt(Math.max(1, Math.round(Number(anchor.rate) * (ratioNow / anchor.ratio))));
      const candGas = anchor.gasObs !== "0" && gasObs > 0n
        ? BigInt(Math.max(1, Math.round(Number(anchor.gas) * (Number(gasObs) / Number(anchor.gasObs)))))
        : BigInt(anchor.gas);
      const drift = (a, b) => (b === 0n ? 10_000n : ((a > b ? a - b : b - a) * 10_000n) / b);
      const driftBps = Math.max(Number(drift(candRate, cur.rate)), Number(drift(candGas, cur.gas)));

      log(name, `domínio ${domain} (${remote.coin}): vigente rate=${cur.rate} gas=${cur.gas} · candidato rate=${candRate} gas=${candGas} · drift=${driftBps}bps`);
      if (driftBps < MIN_CHANGE_BPS) {
        log(name, `domínio ${domain}: estável (<${MIN_CHANGE_BPS}bps) — sem submissão`);
        continue;
      }
      if (DRY) {
        log(name, `domínio ${domain}: [dry-run] submeteria rate=${candRate} gas=${candGas}`);
        continue;
      }
      submitter ??= await makeSubmitter(name, chain);
      const tx = await submitter.submit(domain, candRate, candGas);
      log(name, `domínio ${domain}: submetido → ${tx} (operador ${submitter.sender})`);
    } catch (err) {
      // erro num domínio não impede os demais
      log(name, `domínio ${domain}: ERRO — ${err.message}`);
    }
  }
}

async function round() {
  const chains = Object.entries(config.chains).filter(([, c]) => c.enabled);
  const coins = new Set();
  for (const [, c] of chains) {
    coins.add(c.localCoin);
    for (const r of Object.values(c.remotes)) coins.add(r.coin);
  }
  let usd;
  try {
    usd = await fetchUsdPrices(config.coingecko, [...coins]);
  } catch (err) {
    console.error(`[agent] falha ao buscar preços: ${err.message} — rodada abortada`);
    return;
  }
  console.log(`[agent] preços USD: ${JSON.stringify(usd)}`);

  // chains em paralelo; cada uma é independente
  await Promise.allSettled(chains.map(([name, c]) => runChain(name, c, usd)));

  // fase de CLAIMS (sequencial — poupa os RPCs públicos); estado no state.json
  for (const [name, c] of chains) {
    await runClaims(name, c, state, DRY, config.epochDurationSecs ?? 21_600);
  }
  if (!DRY) saveState();
}

console.log(`[agent] oracle-agent iniciando · chains: ${Object.keys(config.chains).filter((k) => config.chains[k].enabled).join(", ")}${DRY ? " · DRY-RUN" : ""}`);
await round();
if (!ONCE) {
  const interval = (config.intervalSeconds ?? 3600) * 1000;
  console.log(`[agent] próxima rodada em ${config.intervalSeconds ?? 3600}s (loop)`);
  setInterval(round, interval);
}
