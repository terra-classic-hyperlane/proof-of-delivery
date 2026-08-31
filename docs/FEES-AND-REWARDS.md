# IGP fees and rewards — pass-through model (applied 2026-08-20)

Model: **whoever sends pays the fee at the origin IGP (~US$ 0.08) and the operator's
reward mirrors the corridor fee** — with no fixed value of its own ("what the user
paid goes to the operator; their profit is what's left after the gas they spend").
The fee floats with real gas/exchange rates (the oracle-agent updates the oracles every 4h),
so the value in $ drifts with the market; **to re-center at $0.08, run the script
again** (idempotent).

Tool: `deploy/igp-tariff.mjs`
```bash
DRY=1 node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol   # check
TC_PRIVATE_KEY=… BSC_PRIVATE_KEY=0x… ETH_PRIVATE_KEY=0x… \
  node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol      # apply
# TARGET_USD=0.10 changes the target (default 0.08)
```

Prices used in this application run: **LUNC $0.00005051 · BNB $625.11 · SOL $84.84 ·
ETH $2,256.75** · target **$0.08/send**.

> The "gas charged"/overhead is only a **billing unit** (quote = gas × gas_price ×
> exchange_rate from the oracle) — the relayer does not spend this gas; it pays the real gas of the
> delivery at the destination.

## Receipts pay REAL GAS, not the fee (migrate of 2026-08-20)

### Where the $0.08 markup lives on each chain (why only TC needed a fix)

| Chain | Where the fee is | Who passes through it |
|---|---|---|
| BSC / ETH | **custom IGP** (`TerraClassicIGPStandalone`) — hook **only for the warp** | user transfers only |
| Solana | **overhead IGP** (`FXacR73…`) — **only the warp** references it | user transfers only |
| **TC** | **shared IGP** (mailbox default hook) | **everything** leaving TC — warp AND receipts |

On the remote chains the receipts use the chain's **official mailbox** (official, cheap hook) —
they never touch our custom IGP. On-chain proof: the TC→BSC receipt emitted on
08-20 AFTER the new fee (tx `0x445eda568614871322c067757f6a996b554b1accdbbed656e263f4f76e5a95a9`)
paid **value = 0** + only the tx gas (~$0.006).

On TC, without adjustment, the RECEIPTS would also pay $0.08 and would devour the
commission (BSC→TC and SOL→TC became **negative** — measured P&L: −$0.008 and −$0.002).
Fix (code_id **11596**, migrate `53ACFEC1…`, same address, pool
preserved, `layout_check ok`): `SendReceipt{gas_limit}` — the vault passes
metadata to the IGP (32B BE of the gas + empty refund → refund = the pool) and the receipt
pays only the real delivery gas. The warp does not expose metadata → the user keeps
paying $0.08, with no hole in the fee. The claim-agent quotes the IGP dynamically
(`quote_gas_payment` of the `gas_limit`, +2% margin; env `RECEIPT_GAS_56` /
`RECEIPT_GAS_SOL`) — no fixed values. Migration:
`deploy/archive/tc-migrate-vault-gas-recibo.sh`.

### Receipt map per corridor (post-migrate)

| Corridor | Receipt leaves from | IGP that charges | Receipt cost |
|---|---|---|---|
| TC→BSC | BSC (vault `0x34E06a…`) | BSC official mailbox | $0 + gas ~$0.006 ✓ proven |
| TC→ETH | ETH (vault does not exist yet) | ETH official mailbox | same, once it exists |
| BSC→TC | TC (vault `terra1gqkrh2…`) | our IGP, 300k gas via metadata | ~100 LUNC (~$0.005) |
| SOL→TC | TC (vault `terra1gqkrh2…`) | our IGP, 500k gas via metadata | ~20 LUNC (~$0.001) |
| TC→SOL | no receipt (epoch quorum on Solana) | — | ~$0.0001 (report tx) |

**Operator P&L per transfer** (prices of 08-20): TC→SOL +$0.079 ·
TC→BSC +$0.065 · SOL→TC +$0.075 · BSC→TC +$0.067 — all corridors
profitable and automatic (no "only send when it's worth it" logic).

---

## Terra Classic (columbus-5, domain 132556) — origin TC→ETH/BSC/SOL

| Piece | Address |
|---|---|
| **IGP** | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP Oracle (StorageGasOracle) | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` (owner = oracle-governor `terra1z7jmlky…9sv4hj`) |
| Vault (rewards) | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` |
| Owner (IGP/vault) | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |

**Fee** — `SetGasForDomain` tx `91883150EC580BDA624B0EBA0BBD9B8365D2C24CDA6352216DB46AC8FE226402`
(the `default_gas` stays 100,000; cw format: `u128` in JSON is a **string**):

| Destination | Gas charged (before → now) | Quote in the application |
|---|---|---|
| 1 (Ethereum) | 100,000 → **5,394,480** | ≈ 1,583.8 LUNC = $0.08 |
| 56 (BSC) | 100,000 → **4,803,897** | ≈ 1,583.8 LUNC = $0.08 |
| 1399811149 (Solana) | 100,000 → **40,158,741** | ≈ 1,583.8 LUNC = $0.08 |

**Rewards** (vault, paid in LUNC from the TC pool — they mirror the fee):

| Knob | Value | tx |
|---|---|---|
| `remote_reward[1]` (TC→ETH deliveries) | **1,583,844,760 uluna** ($0.08) | `6D8CB3A1E8BBF6CB5E123ACB4CFAFB3ECCF9CBCED3502EEC2B32448287248726` |
| `remote_reward[56]` (TC→BSC deliveries) | **1,583,844,841 uluna** ($0.08) | `43970286BFB43FC146227DBBFB87764BB9706A1C52E19479CCB908E4F3DE11C2` |
| `reward_per_delivery` (direct claim) | 1 uluna (deliberately disabled — the receipt model covers it) | — |

## BSC (domain 56) — origin BSC→TC

| Piece | Address |
|---|---|
| **IGP** (TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` (owner = GasOracleGovernor `0x5CF7A3a7…13E5`) |
| Receipt vault (rewards) | `0x34E06a7793877EC5251b1dC230aD7cD577d231f4` |
| Owner | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

- **Fee**: `gasOverhead` 200,000 → **14,159,690** — tx `0x424e95c65d9ec56139bf3f230ef51917fd5a0f7c93c496b6748716e52dc06042` · quote verified after: **$0.08**.
- **Reward**: `remoteReward[132556]` (BSC→TC deliveries, paid in BNB) = **127,977,472,971,950 wei** (0.000128 BNB = $0.08) — tx `0xfd57684606ed28f43f7c19d3092a16a7652265d5b543e19c295cc9dc6f111992`.
- `rewardPerDelivery` (direct claim) stays 5e13 wei (0.00005 BNB) — not changed.

## Ethereum (domain 1) — origin ETH→TC

| Piece | Address |
|---|---|
| **IGP** (TerraClassicIGPStandalone) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` (owner = GasOracleGovernor `0xa1803b36…fEFA`) |
| Owner | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

- **Fee**: `gasOverhead` 300,000 → **1,469,432** — tx `0x50a2fcdc224804997b2b31562e8c0eb75d83bab2b865b68c41f80d245196c42b` · quote verified after: **$0.08**.
- **ETH→TC reward**: the ETH receipt vault **has not yet been deployed** (waiting for
  low gas). The reward for **TC→ETH** deliveries is already paid on TC (`remote_reward[1]`).
  When the ETH vault exists: run `igp-tariff.mjs --rewards --eth` (and include the vault in the script).

## Solana (domain 1399811149) — origin SOL→TC

| Piece | Address |
|---|---|
| **IGP program** | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` |
| IGP inner (accumulates the lamports; proof-of-delivery beneficiary/owner) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| **Overhead IGP** (the one the warp uses) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| pod (vault+governor, rewards) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` (rrv config/pool `Eq1mJGTS…wb9w`) |
| Owner / authority | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

- **Fee**: `SetDestinationGasOverheads(132556)` 3,000,000 → **8,660,148** (intrinsic warp gas ~3M → total ~11.66M) — tx `gqPF86BQetpApYz7VWpX5MHSEBvJXJqnPQAoNxcefuh3PkBERM2hiiFi5PN9SQyrtXTqHTerairGgKxLhadjUFa`.
- **Rewards** (paid in SOL from the pod's pool):

| Knob | Value | tx |
|---|---|---|
| `SetRemoteReward(132556)` (Solana→TC deliveries, receipt) | **942,951 lamports** ($0.08) | `4p5XwNvf3sLmL2Yms6nvLsaUUi5w4gGakiyYF6UTinyVNnq2itS5Fkx5WWSar6kcZ9d5GLrUf5tQJ6o7T7QAqCM6` |
| `SetRewardLamports` (TC→Solana deliveries, epoch quorum) | **942,951 lamports** ($0.08) | `2BGncqeHeuHNmz7QdAikvays4upfALvHeeGVSBEfVbGTcAXzNC3qdp3gbekRPc1u26v3BiTnnHAMTf1pNqCyiPWS` |

---

## Summary per corridor (in the application, 2026-08-20)

| Corridor | Sender pays (origin) | Operator receives (where) |
|---|---|---|
| TC→ETH | ~1,584 LUNC ($0.08) | 1,583,844,760 uluna on TC |
| TC→BSC | ~1,584 LUNC ($0.08) | 1,583,844,841 uluna on TC |
| TC→Solana | ~1,584 LUNC ($0.08) | 942,951 lamports on Solana |
| BSC→TC | 0.000128 BNB ($0.08) | 127,977,472,971,950 wei on BSC |
| ETH→TC | ~0.0000354 ETH ($0.08) | (ETH vault pending) |
| Solana→TC | ~942,951 lamports ($0.08) | 942,951 lamports on Solana |

**Maintenance**: readjust when the $ drifts (e.g., monthly, or if any token moves >2×):
`node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol`. The pool stays neutral in the
same-token corridors (collects X, pays X); in the cross TC↔Solana the conversion uses the
day's exchange rate and the pools absorb small drifts.

## rrv A+B deploy — MAINNET (2026-08-20) ✅

Upgrade of the pod `2mQZcHYL…` with the bitmap replay guard + close/refund of the epoch
rent. Done in production:
- extend +10240 B (the new binary is larger) → deploy resumed from the buffer after an
  RPC outage (swap tx `3nKJAXQU…`, slot 440540031). The 1.65 SOL of the buffer WERE RETURNED to
  the authority (2.266 SOL after).
- migration `SetAppliedBase(82487)` = current epoch−256 (tx `5Dg6zpgL…`); applied_base
  confirmed 82487 on-chain.
- Config intact post-upgrade (quorum 1, reward 942951, 2 operators, total_credited
  6000000); commission pool intact (0.3006 SOL). The new code reads the old migrated
  Config correctly.
- Cost of the TC→Solana report: from US$1.29/epoch (permanent rent) → ~US$0 (refundable).

Scripts: `deploy/archive/solana-upgrade-pod.sh` (+`archive/solana-resume-upgrade.sh` for unstable RPC) ·
`deploy/archive/rrv-migrate-applied-base.mjs`.
