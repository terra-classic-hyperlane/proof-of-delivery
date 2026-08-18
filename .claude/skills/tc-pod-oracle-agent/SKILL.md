---
name: tc-pod-oracle-agent
description: >-
  Runbook do oracle-agent (Node) — o feed de preço multi-chain dos governors
  (Terra Classic, BSC, Ethereum, Solana). Use ao configurar, operar, depurar ou
  estender o agente (nova chain, nova fonte de preço/gás, cron do operador).
---

# oracle-agent (runbook)

> Código: `oracle-agent/` · README próprio com uso. O agente NÃO tem poder:
> quórum + mediana + faixa + delta são aplicados on-chain pelos governors.

## O que faz por rodada
1. Busca preços USD (CoinGecko, 1 chamada para todos os tokens);
2. Por chain local habilitada × domínio remoto:
   `token_exchange_rate = preço(remoto)/preço(local) × SCALE` —
   **SCALE por VM da chain LOCAL**: 1e10 (cosmwasm/evm) · **1e19 (solana)**;
3. Gas do remoto: `eth_gasPrice` via RPC (`evm-rpc`) ou `fixed`;
4. Submete `SubmitPrice` no governor local:
   - TC: execute CosmWasm `{"submit_price":{...}}` (cosmjs, `TC_MNEMONIC`);
   - EVM: `governor.submitPrice(domain, rate, gas)` (ethers, `EVM_PRIVATE_KEY`);
   - Solana: instrução borsh variante 1, PDAs `gov-config` / `gov-domain-{u32le}` /
     `gov-price-{domain}-{epoch}` (`SOLANA_KEYPAIR_PATH`).

## Configuração
`config.example.json` → `config.json`. Chave NUNCA no arquivo — os campos
`mnemonicEnv/privateKeyEnv/keypairEnv` dizem QUAL env ler. Domain do TC: **132556**.
Nova chain = nova entrada em `chains` (tipos: cosmwasm | evm | solana) + coin no
`coingecko.ids`. Nova fonte de gás = novo `type` em `fetchRemoteGasPrice` (prices.js).

## Comandos
```bash
npm test         # matemática de escala (node:test)
npm run dry-run  # rodada real de cotação SEM assinar — use antes de qualquer mudança
npm run once     # 1 rodada assinada (cron recomendado: a cada época de 6h + jitter)
npm start        # loop contínuo (intervalSeconds)
```

## Depuração
- Erros são POR DOMÍNIO (um falhar não derruba os outros) — ler o log da rodada;
- Submissão rejeitada on-chain: conferir se é `NoBounds` (governança não definiu
  faixa), `OutOfBounds` (checar fonte de preço), `EpochAlreadyApplied` (outro
  operador fechou o quórum — normal) ou `Delta` (movimento > bps → emergência);
- `dry-run` compara bem com o esperado: LUNC ~5e-5 USD → ETH em LUNC ≈ 4e17 na
  escala 1e10. Ordem de grandeza muito diferente = fonte de preço errada.

## Operação multi-operador
Cada operador roda o SEU agente com a SUA chave, sem coordenação — o governor
converge pela mediana (menor dos centrais). Não compartilhar chave entre operadores:
isso colapsa o quórum em 1 entidade (mesmo limiar de confiança do ISM).
