---
name: tc-pod-oracle-agent
description: >-
  Runbook for the oracle-agent (Node) — the multi-chain price feed for the governors
  (Terra Classic, BSC, Ethereum, Solana). Use when configuring, operating, debugging or
  extending the agent (new chain, new price/gas source, operator cron).
---

# oracle-agent (runbook)

> Code: `oracle-agent/` · its own README with usage. The agent has NO power:
> quorum + median + bounds + delta are enforced on-chain by the governors.

## What it does per round
1. Fetches USD prices (CoinGecko, 1 call for all tokens);
2. Per enabled local chain × remote domain:
   `token_exchange_rate = price(remote)/price(local) × SCALE` —
   **SCALE per VM of the LOCAL chain**: 1e10 (cosmwasm/evm) · **1e19 (solana)**;
3. Remote gas: `eth_gasPrice` via RPC (`evm-rpc`) or `fixed`;
4. Submits `SubmitPrice` on the local governor:
   - TC: execute CosmWasm `{"submit_price":{...}}` (cosmjs, `TC_MNEMONIC`);
   - EVM: `governor.submitPrice(domain, rate, gas)` (ethers, `EVM_PRIVATE_KEY`);
   - Solana: borsh instruction variant 1, PDAs `gov-config` / `gov-domain-{u32le}` /
     `gov-price-{domain}-{epoch}` (`SOLANA_KEYPAIR_PATH`).

## Configuration
`config.example.json` → `config.json`. Key NEVER in the file — the fields
`mnemonicEnv/privateKeyEnv/keypairEnv` say WHICH env to read. TC domain: **132556**.
New chain = new entry in `chains` (types: cosmwasm | evm | solana) + coin in
`coingecko.ids`. New gas source = new `type` in `fetchRemoteGasPrice` (prices.js).

## Commands
```bash
npm test         # scale math (node:test)
npm run dry-run  # real quoting round WITHOUT signing — use before any change
npm run once     # 1 signed round (recommended cron: every 6h epoch + jitter)
npm start        # continuous loop (intervalSeconds)
```

## Debugging
- Errors are PER DOMAIN (one failing does not bring down the others) — read the round log;
- Submission rejected on-chain: check whether it is `NoBounds` (governance did not set
  bounds), `OutOfBounds` (check price source), `EpochAlreadyApplied` (another
  operator closed the quorum — normal) or `Delta` (movement > bps → emergency);
- `dry-run` compares well with the expected: LUNC ~5e-5 USD → ETH in LUNC ≈ 4e17 at
  scale 1e10. A very different order of magnitude = wrong price source.

## Multi-operator operation
Each operator runs THEIR agent with THEIR key, without coordination — the governor
converges via the median (lower of the central ones). Do not share a key between operators:
that collapses the quorum into 1 entity (same trust threshold as the ISM).
