// oracle-agent — the operator's off-chain side (spec §03/§10, generalized):
// at each interval, it observes price (CoinGecko) and gas of the remote domains and
// submits SubmitPrice to the GOVERNOR of each enabled local chain.
//
// The agent does NOT decide anything on its own: quorum, median, bounds and delta are
// applied on-chain by the governors. An error on one chain does not bring down the others.
//
// Usage:  node src/index.js [--once] [--dry-run] [--config path.json]
//   --dry-run: computes and prints what it would submit, without signing anything
//   --once:    a single round (useful in cron); without it, loop with interval

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
  console.warn(`[agent] config.json not found — using ${path.basename(examplePath)} (only makes sense with --dry-run)`);
}

const log = (chain, msg) => console.log(`[${new Date().toISOString()}] [${chain}] ${msg}`);

// ---------------------------------------------------------------------------
// ANCHOR MODE (production is the truth): instead of computing the rate from scratch (each
// deployment has its own calibration!), the agent anchors to the current ON-CHAIN
// value on the first round and, afterwards, only ADJUSTS it by the RELATIVE variation of
// the price (rate) and of the observed gas (gas). Quorum/bounds/delta stay on-chain.
// Manually recalibrated the oracle? Delete the entry in state.json → it re-anchors.
// ---------------------------------------------------------------------------
const statePath = config.statePath ?? path.join(root, "state.json");
const state = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, "utf8")) : {};
const saveState = () => fs.writeFileSync(statePath, JSON.stringify(state, null, 2));
const MIN_CHANGE_BPS = config.minChangeBps ?? 300; // only submits if drift >= 3%

const mods = {
  cosmwasm: () => import("./chains/terraclassic.js"),
  evm: () => import("./chains/evm.js"),
  solana: () => import("./chains/solana.js"),
};

async function makeSubmitter(name, chain) {
  const mod = await mods[chain.type]?.();
  if (!mod) throw new Error(`[${name}] unknown type: ${chain.type}`);
  switch (chain.type) {
    case "cosmwasm": return mod.makeCosmwasmSubmitter(chain);
    case "evm": return mod.makeEvmSubmitter(chain);
    case "solana": return mod.makeSolanaSubmitter(chain, config.epochDurationSecs ?? 21_600);
  }
}

async function runChain(name, chain, usd) {
  const mod = await mods[chain.type]?.();
  if (!mod) throw new Error(`[${name}] unknown type: ${chain.type}`);
  let submitter = null; // created only when there is something to submit (dry-run needs no key)
  for (const [domain, remote] of Object.entries(chain.remotes)) {
    try {
      const cur = await mod.readOracle(chain, domain); // CURRENT on-chain
      const ratioNow = usd[remote.coin] / usd[chain.localCoin];
      const gasObs = BigInt(await fetchRemoteGasPrice(remote.gasPriceSource));
      const key = `${name}:${domain}`;
      const anchor = state[key];

      if (!anchor) {
        log(name, `domain ${domain} (${remote.coin}): anchor ${DRY ? "would be created" : "created"} at current rate=${cur.rate} gas=${cur.gas} (ratio=${ratioNow.toExponential(4)}) — nothing submitted`);
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

      log(name, `domain ${domain} (${remote.coin}): current rate=${cur.rate} gas=${cur.gas} · candidate rate=${candRate} gas=${candGas} · drift=${driftBps}bps`);
      if (driftBps < MIN_CHANGE_BPS) {
        log(name, `domain ${domain}: stable (<${MIN_CHANGE_BPS}bps) — no submission`);
        continue;
      }
      if (DRY) {
        log(name, `domain ${domain}: [dry-run] would submit rate=${candRate} gas=${candGas}`);
        continue;
      }
      submitter ??= await makeSubmitter(name, chain);
      const tx = await submitter.submit(domain, candRate, candGas);
      log(name, `domain ${domain}: submitted → ${tx} (operator ${submitter.sender})`);
    } catch (err) {
      // an error on one domain does not block the others
      log(name, `domain ${domain}: ERROR — ${err.message}`);
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
    console.error(`[agent] failed to fetch prices: ${err.message} — round aborted`);
    return;
  }
  console.log(`[agent] USD prices: ${JSON.stringify(usd)}`);

  // chains in parallel; each one is independent
  await Promise.allSettled(chains.map(([name, c]) => runChain(name, c, usd)));

  // CLAIMS phase — DISCONTINUED (old v2 attestation model). Claims are now
  // done by `claim-agent-receipt.mjs` (receipt) + `solana-epoch-reporter.mjs`
  // (quorum). Only runs if config.doClaims === true (default: OFF) — leaving it on
  // causes "execution reverted" / getLogs failing every round.
  if (config.doClaims === true) {
    const ordered = [...chains].sort(([, a], [, b]) => (a.type === "cosmwasm" ? 1 : 0) - (b.type === "cosmwasm" ? 1 : 0));
    for (const [name, c] of ordered) {
      await runClaims(name, c, state, DRY, config.epochDurationSecs ?? 21_600);
    }
  }
  if (!DRY) saveState();
}

console.log(`[agent] oracle-agent starting · chains: ${Object.keys(config.chains).filter((k) => config.chains[k].enabled).join(", ")}${DRY ? " · DRY-RUN" : ""}`);
await round();
if (!ONCE) {
  const interval = (config.intervalSeconds ?? 3600) * 1000;
  console.log(`[agent] next round in ${config.intervalSeconds ?? 3600}s (loop)`);
  setInterval(round, interval);
}
