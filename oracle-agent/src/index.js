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

async function makeSubmitter(name, chain) {
  if (DRY) {
    return { sender: "(dry-run)", submit: async () => "(dry-run)" };
  }
  switch (chain.type) {
    case "cosmwasm": {
      const { makeCosmwasmSubmitter } = await import("./chains/terraclassic.js");
      return makeCosmwasmSubmitter(chain);
    }
    case "evm": {
      const { makeEvmSubmitter } = await import("./chains/evm.js");
      return makeEvmSubmitter(chain);
    }
    case "solana": {
      const { makeSolanaSubmitter } = await import("./chains/solana.js");
      return makeSolanaSubmitter(chain, config.epochDurationSecs ?? 21_600);
    }
    default:
      throw new Error(`[${name}] type desconhecido: ${chain.type}`);
  }
}

async function runChain(name, chain, usd) {
  const submitter = await makeSubmitter(name, chain);
  for (const [domain, remote] of Object.entries(chain.remotes)) {
    try {
      const rate = exchangeRate(usd[remote.coin], usd[chain.localCoin], chain.type);
      const gasPrice = await fetchRemoteGasPrice(remote.gasPriceSource);
      log(
        name,
        `domínio ${domain} (${remote.coin}): exchange_rate=${rate} gas_price=${gasPrice}` +
          (DRY ? " [dry-run: não submetido]" : ""),
      );
      if (!DRY) {
        const tx = await submitter.submit(domain, rate, gasPrice);
        log(name, `domínio ${domain}: submetido → ${tx}`);
      }
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
}

console.log(`[agent] oracle-agent iniciando · chains: ${Object.keys(config.chains).filter((k) => config.chains[k].enabled).join(", ")}${DRY ? " · DRY-RUN" : ""}`);
await round();
if (!ONCE) {
  const interval = (config.intervalSeconds ?? 3600) * 1000;
  console.log(`[agent] próxima rodada em ${config.intervalSeconds ?? 3600}s (loop)`);
  setInterval(round, interval);
}
