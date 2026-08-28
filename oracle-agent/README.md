# oracle-agent

The **off-chain side of the relayer operator**: it watches the native-token
prices (CoinGecko) and the gas price of the remote domains, and submits
`SubmitPrice` to the **governor of each chain** of the bridge — Terra Classic
(CosmWasm), BSC and Ethereum (EVM) and Solana. It is generic: any new chain is
added through configuration.

The agent **has no power**: the on-chain governor is what applies values, with
a quorum among operators, the **median** (lower of the central pair), a
**range** defined by governance/multisig and a **maximum delta** per epoch. A
compromised agent, at worst, submits a number the other operators do not
confirm.

## How it works

1. Each round (default 1h): fetches the USD prices of all tokens at once;
2. For each enabled local chain, for each remote domain:
   `token_exchange_rate = price(remote)/price(local) × scale` — **scale per VM**:
   `1e10` (CosmWasm/EVM) and `1e19` (Solana), as spec §08 requires;
3. Remote gas price: `eth_gasPrice` via RPC (EVM) or a configured fixed value;
4. Submits to the local chain's governor (CosmWasm execute · EVM `submitPrice` ·
   Solana borsh instruction with the derived PDAs).

## Usage

```bash
npm install
cp config.example.json config.json   # fill in governors/RPCs/domains

# see what would be submitted, without signing anything:
npm run dry-run

# one real round (good for cron):
TC_MNEMONIC="..." EVM_PRIVATE_KEY="0x..." SOLANA_KEYPAIR_PATH=~/keypair.json npm run once

# continuous loop:
npm start
```

Keys only via environment variables (`mnemonicEnv` / `privateKeyEnv` /
`keypairEnv` in the config say WHICH env to read) — no secrets in configuration
files.

## Tests

```bash
npm test    # node:test — exchange_rate math and per-VM scales
```

## Operation (multiple operators)

Each operator runs **their own** agent with **their own** key. There is no
coordination: everyone observes the market independently and the governor
converges through the median. A cron per epoch (6h) with a small per-operator
jitter is recommended.
