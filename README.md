# tc-proof-of-delivery

Remuneration of relayers on the Terra Classic Hyperlane bridge
(**Terra Classic · BSC · Ethereum · Solana**). Each network has a **Vault** as
the beneficiary of the local IGP: users pay interchain gas, fees flow into the
vault pool, and the operator gets paid for what it **provably delivered** —
proven by the chain's own execution record (TC via raw storage query, EVM via
`processor()`, Solana via epoch quorum). Gas prices are proposed by the
**oracle-agent** and applied only after a **quorum of validator operators**
approves.

> **Start here:** 📖 [org documentation hub](https://github.com/terra-classic-hyperlane/docs) ·
> 🛠️ [`docs/install/INSTALL.md`](docs/install/INSTALL.md) (operator install) ·
> 📋 [`docs/install/AUDIT.md`](docs/install/AUDIT.md) (contracts, hashes, powers, verification)

---

## 1. What is in this repo (and what compiles)

| Path | Stack | Contents | Compile / test |
|---|---|---|---|
| [`contracts/`](contracts/) | CosmWasm (Rust workspace) | `relayer-reward-vault` (the TC vault) · `oracle-governor` (quorum-gated gas-price governance) | `cargo check --workspace` — reproducible artifacts: §5 |
| [`evm/`](evm/) | Solidity (Foundry) | `RelayerRewardVault.sol` · `GasOracleGovernor.sol` + unit tests | `cd evm && forge build && forge test` |
| [`svm/`](svm/) | Solana (Rust workspace) | programs `pod` (proof-of-delivery / receipts) · `relayer-reward-vault` · `igp-oracle-governor` · `mock-igp` (test-only mock) | `cd svm && cargo check` (deployable `.so` via `cargo build-sbf`) |
| [`oracle-agent/`](oracle-agent/) | Node.js | the gas-price oracle agent (proposes prices on the 4 networks; quorum applies them) | `cd oracle-agent && npm ci` |
| [`deploy/`](deploy/) | Node.js / bash | production agents, deploy scripts and admin tooling (§3–§4) | `cd deploy && npm ci` |
| [`docs/`](docs/) | — | operator/validator guides, architecture, audit — see §6 | — |
| `artifacts/`, `deploy/*.state` | — | built wasm + deployment state records (read by the scripts to resume/skip) | do not edit |

All three contract stacks compile clean as of 2026-08-31.

## 2. Production services (what actually runs)

Four systemd services run in production, all from this repo:

| Service | Entry point | What it does |
|---|---|---|
| `oracle-agent` | `oracle-agent/src/index.js` | updates the gas oracles on the 4 networks every 4 h — proposes via the governor; a validator quorum approves ([docs/ORACLE-AGENT.md](docs/ORACLE-AGENT.md)) |
| `claim-agent` | `deploy/claim-agent-receipt.mjs --loop 300` | issues receipts and sweeps/claims commissions (runs on the tooling wallet, not the relayer wallet) |
| `epoch-reporter` | `deploy/solana-epoch-reporter.mjs --submit --loop 3600` | TC→Solana epoch quorum reporting |
| `deliver-receipts` (timer, plan B) | `deploy/deliver-receipts-tc.mjs` | safety net: delivers BSC→TC receipts stuck > 30 min (the official relayer is primary) |

Related keeper (run on demand): `deploy/solana-receipt-keeper.mjs` — delivers a
pending TC→Solana message and dispatches its receipt in one atomic transaction.

## 3. Configuration

| File | Configures |
|---|---|
| [`oracle-agent/config.example.json`](oracle-agent/config.example.json) | the oracle-agent: RPCs, contracts per network, `originSenders` (add every new warp sender here so its deliveries are swept), intervals — copy to `config.json` |
| [`deploy/rpc.env`](deploy/rpc.env) | RPC endpoints used by the deploy/admin scripts |
| [`deploy/topup.env.example`](deploy/topup.env.example) | auto-topup thresholds/wallets (optional service) |
| systemd units | see [`docs/install/INSTALL.md`](docs/install/INSTALL.md) and `deploy/install-operator.sh` — the one-shot operator installer |

Keys are provided via environment only (never in files). The claim/receipt
tooling signs with a **separate wallet** from the relayer to avoid account
sequence contention.

## 4. Execution — common operations

```bash
cd deploy && npm ci

# Become an operator (one-shot installer: agents + systemd units)
bash install-operator.sh

# Register as a Solana operator / adjust bounds
node register-solana-operator.mjs
node raise-bounds.mjs

# Vault administration (owner/multisig)
node rrv-admin.mjs                  # inspect/administer the vaults
node rrv-set-reward.mjs             # reward = the tariff (pass-through, never fixed)
node rrv-remote-config.mjs          # remote/router configuration
node rrv-withdraw-operator.mjs      # operator withdrawal

# IGP tariff (origin-side, ~USD 0.08 pass-through)
node igp-tariff.mjs                 # see docs/FEES-AND-REWARDS.md

# ISM validator rotation (mutable ISMs — one owner tx per chain)
node update-ism-validators.mjs      # see docs/ISM-VALIDATORS.md
node storage-ism.mjs                # deploy a mutable StorageMessageIdMultisigIsm (EVM)

# Diagnostics / monitoring
node solana-quem-entregou.mjs       # who delivered a given message
node monitor.mjs                    # CLI monitor · monitor-web.mjs + installer = web version
```

First-time network deploys (already done on mainnet — needed only for a new
environment): `tc-deploy.sh`, `evm-deploy.sh` / `evm-vault-receipt.sh`,
`solana-deploy.sh` + `solana-init.mjs`.

## 5. Reproducible build & audit

- **CosmWasm artifacts** (byte-for-byte `data_hash` match on columbus-5):
  `docker run --rm -v "$(pwd)":/code cosmwasm/optimizer:0.16.0` → `artifacts/`
- **Full audit trail** (addresses, hashes, powers, verify commands):
  [`docs/install/AUDIT.md`](docs/install/AUDIT.md)
- **Historical phase log** (source-code verification + phase-by-phase mainnet
  evidence, 08/2026): [`docs/archive/PHASE-LOG.md`](docs/archive/PHASE-LOG.md)
- Specification: [`SPEC.html`](SPEC.html) (v3)

## 6. Documentation index

| Topic | Doc |
|---|---|
| Architecture (diagrams) | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Install & run everything | [docs/INSTALL-AND-RUN.md](docs/INSTALL-AND-RUN.md) · [docs/install/INSTALL.md](docs/install/INSTALL.md) |
| Vault (how the relayer is paid) | [docs/VAULT.md](docs/VAULT.md) |
| Oracle agent + quorum | [docs/ORACLE-AGENT.md](docs/ORACLE-AGENT.md) · [docs/PROPOSAL-PARAMETERS.md](docs/PROPOSAL-PARAMETERS.md) |
| Fees & rewards (tariff pass-through) | [docs/FEES-AND-REWARDS.md](docs/FEES-AND-REWARDS.md) |
| ISM validator sets & rotation | [docs/ISM-VALIDATORS.md](docs/ISM-VALIDATORS.md) |
| Operators / validators guides | [docs/OPERATORS.md](docs/OPERATORS.md) · [docs/OPERATORS-VALIDATORS-GUIDE.md](docs/OPERATORS-VALIDATORS-GUIDE.md) · [docs/TCV-VALIDATOR-MAINNET-GUIDE.md](docs/TCV-VALIDATOR-MAINNET-GUIDE.md) |
| Trustless receipt / remote claim | [docs/TRUSTLESS-RECEIPT.md](docs/TRUSTLESS-RECEIPT.md) · [docs/REMOTE-CLAIM.md](docs/REMOTE-CLAIM.md) ([security](docs/REMOTE-CLAIM-SECURITY.md)) |
| Contract-level operations | [docs/CONTRACT-OPERATION.md](docs/CONTRACT-OPERATION.md) |
| Expanding to a new chain | [docs/EXPANSION-MANUAL.md](docs/EXPANSION-MANUAL.md) |

## 🗄️ Archive (arquivo morto)

Executed one-time migrations/upgrades and superseded material — reference only,
never re-run: [`deploy/archive/`](deploy/archive/) (vault v2/i18n/gas-recibo/receipt
migrations, Solana pod upgrades, devnet cleanup) and [`docs/archive/`](docs/archive/)
(incl. the [phase log](docs/archive/PHASE-LOG.md) and the discontinued
[IGORFAKE warp map](docs/archive/WARP-IGORFAKE.md) — the live routes are
[LUNC](https://github.com/terra-classic-hyperlane/cw-hyperlane/blob/main/terraclassic/doc/install/WARP-LUNC.md) and
[USTC](https://github.com/terra-classic-hyperlane/cw-hyperlane/blob/main/terraclassic/doc/install/WARP-USTC.md)).
