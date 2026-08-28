# Operator Install Guide — oracle-agent · claim-agent · epoch-reporter

> How to install and run the **off-chain operator services** of tc-proof-of-delivery
> on a VPS, in one shot. This is one of the two entry-point documents of the project —
> the other is the consolidated [AUDIT.md](AUDIT.md).

---

## 1. Architecture

### 1.1 Component chart (organogram) — who runs what, where

```mermaid
graph TB
    subgraph VPS["Operator VPS (off-chain)"]
        REL["hyperlane-relayer<br/>(delivers messages, earns rewards)"]
        VAL["hyperlane-validator<br/>(signs checkpoints → S3)"]
        OA["oracle-agent<br/>loop 4h — gas/price oracle"]
        CA["claim-agent<br/>loop 5min — receipts & commissions"]
        ER["epoch-reporter<br/>loop 1h — TC→Solana epoch quorum"]
    end

    subgraph TC["Terra Classic (domain 132556)"]
        TCG["oracle-governor<br/>code_id 11587"]
        TCV["relayer-reward-vault<br/>code_id 11635 (the POOL)"]
        TCI["IGP → beneficiary = vault"]
        TCM["Mailbox"]
    end

    subgraph BSC["BSC (domain 56)"]
        BG["GasOracleGovernor"]
        BV["RelayerRewardVault (receipt)"]
        BM["Mailbox + IGP"]
    end

    subgraph ETH["Ethereum (domain 1)"]
        EG["GasOracleGovernor"]
        EV["RelayerRewardVault v2"]
        EM["Mailbox + IGP"]
    end

    subgraph SOL["Solana (domain 1399811149)"]
        POD["pod program<br/>(vault + governor merged)"]
        SM["Mailbox + IGP"]
    end

    OA -->|SubmitPrice| TCG & BG & EG & POD
    CA -->|sendReceipt / claim| TCV & BV
    ER -->|SubmitEpochReport| POD
    REL --> TCM & BM & EM & SM
    TCG -->|median → SetGasPrice| TCI
```

**Roles.** The relayer delivers bridge messages and is paid by the vaults; the three
services in this guide are its **support crew**: `oracle-agent` keeps the gas oracles
honest (so the IGP charges the right fee), `claim-agent` turns deliveries into receipts
and collects the rewards, `epoch-reporter` closes Solana epochs. Whoever operates the
relayer runs all of them.

### 1.2 Flow chart — how a delivery becomes a payment

```mermaid
sequenceDiagram
    participant U as User
    participant OW as Origin chain (Warp+IGP)
    participant R as Relayer
    participant DM as Destination Mailbox
    participant CA as claim-agent
    participant DV as Destination Vault
    participant OV as Origin Vault (pool)

    U->>OW: transfer + IGP gas fee (~$0.08)
    OW-->>R: message dispatched (fee → IGP → vault pool)
    R->>DM: deliver message (pays destination gas)
    CA->>DV: sendReceipt(message_ids)
    DV-->>OV: receipt via bridge (validated by the warp ISM validators)
    OV->>R: pays reward = origin tariff (pass-through, never fixed)
```

And the oracle price flow that keeps the tariff correct:

```mermaid
flowchart LR
    CG["CoinGecko<br/>USD prices"] --> OA["oracle-agent"]
    RPC["eth_gasPrice<br/>remote RPCs"] --> OA
    OA -->|"SubmitPrice(domain, rate, gas)"| GOV["governor (each chain)"]
    GOV -->|"median of operators<br/>+ range + max delta/epoch"| ORC["IGP oracle"]
    ORC --> IGP["IGP quotes the user's gas fee"]
```

The agent **has no power**: the governor applies the **median** of all operators'
submissions, clamped to governance-defined **ranges** and a **max delta per epoch**.
A compromised agent at worst submits a number the others do not confirm.

---

## 2. Prerequisites

| Item | Requirement |
|---|---|
| VPS | 2 vCPU / 4 GB RAM / 40 GB disk, Ubuntu 22.04+ |
| Node.js | **v20+** (`node -v`) |
| Repo | `git clone https://github.com/terra-classic-hyperlane/proof-of-delivery` |
| RPCs | defaults in `rpc.env` work (Hexxagon TC, publicnode BSC/ETH, Solana mainnet) |
| relayer/validator | separate install — see `../RELAYER-VPS.md` (not covered here) |

### Wallets & funding

Use a **tooling wallet** for these services, separate from the relayer wallet
(avoids account-sequence contention). Fund it with small amounts of gas:

| Chain | Used by | Needs |
|---|---|---|
| Terra Classic | claim-agent | LUNC for receipt txs |
| BSC | claim-agent, oracle-agent | BNB (only spends when drift >300bps or receipts pending) |
| Ethereum | oracle-agent | ETH (rarely — only on drift >300bps) |
| Solana | epoch-reporter, oracle-agent | SOL (~0.1; the round rent is paid once per domain) |

---

## 3. One-shot install

```bash
cd proof-of-delivery
bash deploy/install-operator.sh
```

The script (idempotent — re-run it to update code; it **never** touches your
`.env`/`config.json`/`state.json`):

1. Creates `/root/oracle-agent` and `/root/claim-agent` (+ `logs/`) and copies the code;
2. Installs npm dependencies;
3. Creates `config.json` and `.env`/`rpc.env` **templates** if they don't exist;
4. Installs + enables 3 systemd units (`oracle-agent`, `claim-agent`, `epoch-reporter`).

Then fill in the secrets and start:

```bash
nano /root/oracle-agent/.env      # TC/BSC/ETH/SOL private keys (oracle operator)
nano /root/claim-agent/.env       # TC/BSC/SOLANA private keys (tooling wallet)
nano /root/oracle-agent/config.json   # governors/RPCs/domains (defaults = production)
systemctl start oracle-agent claim-agent epoch-reporter
```

Keys are read **only from environment variables** — never stored in config files.

---

## 4. Verify & operate

```bash
# all three must be "active"
systemctl is-active oracle-agent claim-agent epoch-reporter

# live logs
tail -f /root/oracle-agent/logs/agent.log     # "stable (<300bps) — no submission" = healthy
tail -f /root/claim-agent/logs/agent.log      # "receipts pending delivery on TC: 0" = healthy
tail -f /root/claim-agent/logs/reporter.log   # "epoch already reported/open — nothing to do" = healthy

# vault solvency (pool must equal claims_payable)
curl -s "https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/contract/terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q/smart/$(echo -n '{"pool":{}}' | base64 -w0)"
```

Cadence reference: oracle-agent rounds every **4 h**, only submits when drift
> **300 bps**; epoch = **6 h**; governor max delta = **2000 bps** per epoch.

## 5. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `DeltaExceeded` / `out of bounds` in oracle log | real market moved beyond the governor's range — self-heals when gas returns, or governance ForceSet |
| `insufficient lamports` on Solana | fund the operator wallet with SOL |
| RPC timeouts / 429 | swap the RPC in `rpc.env` / `config.json` |
| service `activating` in a loop | `journalctl -u <name> -n 50` — usually an empty env var |

Deep dives: `../ORACLE-AGENT.md` · `../CLAIM-AGENT-INSTALL.md` · `../VAULT.md` ·
`../TRUSTLESS-RECEIPT.md` · `../OPERATOR-INSTALL-RELAYER-ORACLE-VAULT.md` (full node incl. relayer).
