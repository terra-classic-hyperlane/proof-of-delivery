# Operator Installation — Relayer + Oracle + Vault (tc-proof-of-delivery)

> Consolidated guide to bring up an **operator node** for Terra Classic Hyperlane on a VPS.
> **Whoever operates the relayer runs the whole package:** relayer + validator + oracle-agent (gas prices)
> + claim-agent/epoch-reporter (vault receipts and rewards). This doc reflects the **real production**
> setup. Deep dives: `RELAYER-VPS.md`, `ORACLE-AGENT.md`, `VAULT.md`,
> `INSTALACAO-CLAIM-AGENT.md`, `OPERADORES.md`, `INSTALACAO_E_EXECUCAO.md` (contract build/deploy).

---

## 1. What the operator runs (5 systemd services)

| Service | Role | Command (ExecStart) | Cadence |
|---|---|---|---|
| **hyperlane-validator** | signs TC Mailbox checkpoints (S3) | `bin/validator --originChainName terraclassic --checkpointSyncer.type s3` | continuous |
| **hyperlane-relayer** | **delivers** messages between chains | `bin/relayer --db … --metrics :9091` | continuous |
| **oracle-agent** | updates the **gas oracles** on the 4 networks (quorum + bounds) | `node src/index.js` | 4h loop (6h epoch) |
| **claim-agent** | emits **receipts** and collects **commissions** (trigger wallet) | `node claim-agent-receipt.mjs --loop 300` | 5 min |
| **epoch-reporter** | reports **TC→Solana delivery quorum** (reward) | `node solana-epoch-reporter.mjs --submit --loop 3600` | 1h |

> Extra (safety net, optional): `deliver-receipts` (timer) — plan B that delivers BSC→TC receipts
> stuck >3min when the relayer loses the sequence. The relayer is the **primary**.

The operator earns from what it **DELIVERS**: the IGP commission (pass-through) lands in the relayer and
the reward is paid out of the **vault** according to the on-chain proof of delivery. See `TARIFAS-E-RECOMPENSAS.md`.

---

## 2. VPS prerequisites

- Ubuntu 22.04+ (production: 4 vCPU / 8 GB / 80 GB SSD is comfortable). The relayer is the heaviest.
- **Node.js v20+** (production uses v22) — `oracle-agent`/`claim-agent`/`epoch-reporter`.
- **Rust 1.84 + build-sbf** only if you are going to **compile** the contracts (most operators use the
  already-published binaries; see `INSTALACAO_E_EXECUCAO.md §3`).
- Hyperlane binaries (`relayer`, `validator`) — in `/root/hyperlane/bin/`.
- **S3 bucket** (for the validator checkpoints) + AWS credentials.
- **RPCs**: TC (LCD + RPC), BSC, Ethereum, Solana (Helius recommended).

---

## 3. Directory layout (production standard)

```
/root/hyperlane/            # relayer + validator
  bin/{relayer,validator}
  runtime/config/mainnet_config.json   # chains, mailboxes, ISMs, IGP
  .env                                  # keys + AWS/S3 + RUST_LOG
/root/oracle-agent/        # oracle-agent (gas prices)
  src/index.js  src/chains/{terraclassic,evm,solana}.js
  .env                                  # TC/BSC/ETH/SOL_PRIVATE_KEY
  logs/agent.log
/root/claim-agent/         # receipts + rewards
  claim-agent-receipt.mjs  solana-epoch-reporter.mjs  deliver-receipts-tc.mjs
  .env  rpc.env
  logs/{agent,reporter,deliver}.log
```

---

## 4. Wallets (create and FUND before starting)

| Wallet | Where | Used for | Fund with |
|---|---|---|---|
| **Relayer/commission** `terra1run9wz…26mawp` | hyperlane `.env` (`TERRA_PRIVATE_KEY`) | TC delivery gas + **vault owner** + receives commission | LUNC |
| **BSC operator** `0x8f085bAD…5291` | oracle-agent `BSC_PRIVATE_KEY` | submit price to the BSC StorageGasOracle | **BNB** (~0.03) |
| **ETH operator** `0xEF818120…00ae` | oracle-agent `ETH_PRIVATE_KEY` | submit price to the Ethereum oracle | **ETH** (~0.01) |
| **Solana operator** `PbEo7Fn…rrkS` | oracle-agent `SOL_PRIVATE_KEY` | submit price to the Solana governor (quorum) | **SOL** (~0.1) |
| **Solana reserve** `BirXd4Q…DEf1j` | local keypair | Solana operator topup **+ pod upgrade authority** ⚠️ | SOL |

> ⚠️ The reserve `BirXd4Q` is **also the upgrade authority** of the Solana programs — keep this keypair
> **off the VPS** (only on the deploy machine). A reserve is not required to work; the Solana operator
> spends ~0.000005 SOL/epoch (fee only) — the round rent is now paid **once** (2026-08 fix).

---

## 5. Environment variables

**`/root/hyperlane/.env`** (relayer + validator):
```
AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_REGION / S3_BUCKET   # validator checkpoints
VALIDATOR_DB / RELAYER_DB                                            # cache dirs
TERRA_PRIVATE_KEY / BSC_PRIVATE_KEY / ETH_PRIVATE_KEY / SOLANA_PRIVATE_KEY
RUST_LOG=info
```
**`/root/oracle-agent/.env`**: `TC_PRIVATE_KEY`, `BSC_PRIVATE_KEY`, `ETH_PRIVATE_KEY`, `SOL_PRIVATE_KEY`
(the `SOL_PRIVATE_KEY` is the **32-byte ed25519 hex seed**, Hyperlane relayer format).
**`/root/claim-agent/.env`**: `BSC_PRIVATE_KEY`, `TC_PRIVATE_KEY`; **`rpc.env`**: the RPCs.

> `chmod 600` on all `.env` files. Never commit them.

---

## 6. Reference addresses (mainnet)

| | Address | Domain |
|---|---|---|
| TC Mailbox | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` | 132556 |
| TC Oracle Governor | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` | |
| TC IGP | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` | |
| **Vault (relayer reward)** | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` | |
| ValidatorAnnounce | `terra1gtnmdevekgxpvzej3wfy20e2n335gm3muwj6geduxxa86j3x70cq00asmy` | |
| Solana Pod (vault+governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` | 1399811149 |

**TC validators (ISM 3-of-4):** igorveras `71b2b8c3…`, tcv `1afd3d07…`, darksun `e6bb0401…`,
burnitall `5c374754…` (threshold **3**). See `ISM-VALIDADORES.md`.

---

## 7. Step-by-step installation

```bash
# 1) hyperlane binaries + config
mkdir -p /root/hyperlane/{bin,runtime/config}
#   copy bin/{relayer,validator} and runtime/config/mainnet_config.json
#   fill in /root/hyperlane/.env (§5) — chmod 600

# 2) oracle-agent
git clone <repo> /root/src && cp -r /root/src/oracle-agent /root/oracle-agent
cd /root/oracle-agent && npm ci
#   fill in .env (§5); set governors/RPCs/domains in src (TC=132556)

# 3) claim-agent (receipts + rewards)
cp -r /root/src/deploy/* /root/claim-agent/   # .mjs scripts + node_modules
cd /root/claim-agent && npm ci
#   fill in .env + rpc.env

# 4) wallets: FUND them (§4) — LUNC on the relayer, BNB/ETH/SOL on the operators

# 5) systemd services (units in §1) → enable
systemctl enable --now hyperlane-validator hyperlane-relayer oracle-agent claim-agent epoch-reporter
```

**Order matters** only on the 1st deployment of the CONTRACTS (see `INSTALACAO_E_EXECUCAO.md §4` — governor →
oracle ownership in 2 steps → bounds per domain → vault → `IGP.set_beneficiary=vault` → seed pool).
To **operate** (contracts already live), the service order is free.

---

## 8. Operation and monitoring

- **Real-time panel:** `node deploy/monitor-web.mjs` → `http://localhost:8787` (wallets, pools,
  services, validators, RPCs). SSE with auto-refresh.
- **Logs:** `journalctl -u <svc> -f` (relayer/validator) · `tail -f /root/oracle-agent/logs/agent.log` ·
  `/root/claim-agent/logs/{agent,reporter}.log`.
- **Minimum health** (spec / skill `tc-pod-deploy`):
  - `LayoutCheck` on TC (after the Mailbox migrate) — avoids wrong parsing.
  - Vault **solvency**: `{"solvency":{}}` → `claims_payable` vs backlog.
  - Solana epochs without quorum (divergent hashes = alarm).
  - Price **not applied** due to `DeltaExceeded` → consider `ForceSet` via governance/multisig.

### oracle-agent rules (important)
- Runs every **4h**; **epoch = 6h**. Only submits when the **drift > 300 bps**; otherwise "stable".
- **Delta cap = 2000 bps** per submission: if the candidate price varies >20% from the last applied, it is
  **rejected** (protection). It stays stuck until the market returns OR a governance `ForceSet`.
- **Solana:** since 2026-08 the round is **one account per domain** (rent paid 1×, reused each epoch) —
  without the old drain of ~0.0151 SOL/epoch. The `CloseRound` instruction closes orphan rounds and returns the rent.

---

## 9. Troubleshooting (real cases)

| Symptom | Cause | Action |
|---|---|---|
| `insufficient lamports` (Solana) | operator out of SOL | transfer SOL to `PbEo7Fn…` (~0.1) |
| `gas_price delta too large … 2000 bps` | swing >20% | wait for the market or `ForceSet` (governance) |
| relayer `limit exceeded` (BSC RPC) | rate-limit of the public RPC | benign (retry) or switch the RPC |
| EVM submission fails | BSC/ETH operator out of gas | transfer BNB/ETH to the addresses (§4) |
| reporter `429 Too Many Requests` | Solana rate-limit | benign (retry) or dedicated RPC (Helius) |

---

## 10. Security (other people's money)

- Keys only on the server, `.env` `chmod 600`, never commit them.
- Solana programs' **upgrade authority** and contracts' **owner** → **multisig** (3 TC validators
  + 1 non-validator; ISM 3-of-4). While the authority is on a deployer, keep it **off the VPS**.
- Effect-before-registration, strict parsing (`deny_unknown_fields`) and replay guards are invariants
  covered by 91 tests — do **not** break them (see skill `tc-pod-contratos`).
