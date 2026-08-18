// Observação de preço e cálculo do token_exchange_rate.
//
// exchange_rate = preço(nativo REMOTO em USD) / preço(nativo LOCAL em USD) × SCALE
// A escala é da VM da chain LOCAL (spec §08 — "as faixas precisam ser
// recalculadas, não copiadas"):
//   CosmWasm / EVM → 1e10   ·   Solana → 1e19

export const SCALE_BY_VM = {
  cosmwasm: 10n ** 10n,
  evm: 10n ** 10n,
  solana: 10n ** 19n,
};

/** Busca os preços USD de todos os ids de uma vez na CoinGecko. */
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
    if (typeof usd !== "number" || usd <= 0) throw new Error(`sem preço para ${coin} (${id})`);
    out[coin] = usd;
  }
  return out;
}

/**
 * token_exchange_rate como BigInt na escala da VM local.
 * Feito em ponto fixo (1e12 de precisão intermediária) para não perder casas
 * com moedas de preço muito baixo (LUNC ~1e-5 USD).
 */
export function exchangeRate(remoteUsd, localUsd, vmType) {
  const scale = SCALE_BY_VM[vmType];
  if (!scale) throw new Error(`vm desconhecida: ${vmType}`);
  const PREC = 10n ** 12n;
  const remote = BigInt(Math.round(remoteUsd * 1e12));
  const local = BigInt(Math.round(localUsd * 1e12));
  if (local === 0n) throw new Error("preço local zero");
  return (remote * scale * PREC) / (local * PREC);
}

/** Gas price do domínio remoto conforme a fonte configurada. */
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
      throw new Error(`gasPriceSource desconhecida: ${source.type}`);
  }
}
