// Price observation and computation of token_exchange_rate.
//
// exchange_rate = price(REMOTE native in USD) / price(LOCAL native in USD) × SCALE
// The scale is that of the LOCAL chain's VM (spec §08 — "the bounds must be
// recomputed, not copied"):
//   CosmWasm / EVM → 1e10   ·   Solana → 1e19

export const SCALE_BY_VM = {
  cosmwasm: 10n ** 10n,
  evm: 10n ** 10n,
  solana: 10n ** 19n,
};

/** Fetches the USD prices of all ids at once from CoinGecko. */
export async function fetchUsdPrices(coingecko, coins) {
  const ids = [...new Set(coins.map((c) => coingecko.ids[c]))].join(",");
  const url = `${coingecko.baseUrl}/simple/price?ids=${ids}&vs_currencies=usd`;
  const res = await fetch(url, { signal: AbortSignal.timeout(15_000) });
  if (!res.ok) throw new Error(`coingecko ${res.status}`);
  const data = await res.json();
  const out = {};
  for (const coin of coins) {
    const id = coingecko.ids[coin];
    const usd = data[id]?.usd;
    if (typeof usd !== "number" || usd <= 0) throw new Error(`no price for ${coin} (${id})`);
    out[coin] = usd;
  }
  return out;
}

/**
 * token_exchange_rate as a BigInt in the local VM's scale.
 * Done in fixed point (1e12 of intermediate precision) so as not to lose digits
 * with very low-priced coins (LUNC ~1e-5 USD).
 */
export function exchangeRate(remoteUsd, localUsd, vmType) {
  const scale = SCALE_BY_VM[vmType];
  if (!scale) throw new Error(`unknown vm: ${vmType}`);
  const PREC = 10n ** 12n;
  const remote = BigInt(Math.round(remoteUsd * 1e12));
  const local = BigInt(Math.round(localUsd * 1e12));
  if (local === 0n) throw new Error("local price zero");
  return (remote * scale * PREC) / (local * PREC);
}

/** Gas price of the remote domain according to the configured source. */
export async function fetchRemoteGasPrice(source) {
  switch (source.type) {
    case "fixed":
      return BigInt(source.value);
    case "evm-rpc": {
      const res = await fetch(source.url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_gasPrice", params: [] }),
        signal: AbortSignal.timeout(10_000),
      });
      if (!res.ok) throw new Error(`gasPrice rpc ${res.status}`);
      const { result } = await res.json();
      return BigInt(result);
    }
    default:
      throw new Error(`unknown gasPriceSource: ${source.type}`);
  }
}
