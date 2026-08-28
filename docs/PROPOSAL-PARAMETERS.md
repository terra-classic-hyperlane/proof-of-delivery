# Starting parameters — proposal based on real costs

Suggested values for the governance proposal, anchored in measurements from
2026-08-18. **Everything here is an adjustable starting point** — governance (TC) and the
multisig (remotes) can recalibrate later; the oracle's delta/bounds and the vault
fee are updatable without a redeploy.

## 0. Evidence used (2026-08-18)

| Measurement | Value | Source |
|---|---|---|
| Real `process()` on TC | **gas_used 508,260 · gas_wanted 655,344** | tx `4126C514…` block 29422362 (mainnet) |
| Minimum gas price on TC | **28.325 uluna/gas** (another tx in the same block: 28.5) | RPC tx_search |
| Gas price BSC | 0.05 gwei | eth_gasPrice publicnode |
| Gas price Ethereum | ~0.22 gwei (historical 0.2–5) | eth_gasPrice publicnode |
| Rent of the `ProcessedMessage` PDA (SOL) | (128+56)×3480×2 ≈ **1,280,640 lamports** ≈ 0.00128 SOL | rent formula + size 56 |
| USD prices | LUNC 0.00004739 · BNB 602.81 · ETH 1,912.92 · SOL 76.84 | CoinGecko (oracle-agent dry-run) |

> ⚠️ **BUG FOUND IN THE CURRENT RELAYER:** the real `process()` tx paid
> **28,325 uluna/gas** (a thousand times the minimum of 28.325) — a fee of 18,562 LUNC
> (~US$ 0.88) on a delivery that should cost ~18.6 LUNC (~US$ 0.0009). Almost
> certainly `gasPrice: "28325uluna"` instead of `"28.325uluna"` in the relayer
> config. **Fix before any sustainability calculation.**

## 1. Real cost per delivery (with the correct gas)

| Network | Calculation | Cost | ≈ USD |
|---|---|---|---|
| Terra Classic | 655,344 gas × 28.325 uluna | ~18.6 LUNC | $0.0009 |
| BSC | ~300,000 gas × 0.05 gwei | 0.000015 BNB | $0.009 |
| Ethereum | ~300,000 gas × 0.22–1 gwei | 0.000067–0.0003 ETH | $0.13–0.57 |
| Solana | rent 1,280,640 + fee ~25,000 lamports | ~0.0013 SOL | $0.10 |

## 2. Fee per delivery (`reward_per_delivery`) — ~2–3× the cost

| Network | Proposal | In minimum units | ≈ USD | Margin |
|---|---|---|---|---|
| Terra Classic | **50 LUNC** | `50000000` uluna | $0.0024 | 2.7× |
| BSC | **0.00005 BNB** | `50000000000000` wei | $0.030 | 3.3× |
| Ethereum | **0.0004 ETH** | `400000000000000` wei | $0.77 | covers up to ~1.3 gwei |
| Solana | **0.003 SOL** | `3000000` lamports | $0.23 | 2.3× |

Solvency invariant: fee < average IGP collection per message.
Monitor with `Solvency`/`claimsPayable` vs backlog; adjust via
governance/quorum if the pool drains.

## 3. Claim window (`claim_window_blocks`) — target ~14 days

| Network | Average block | Proposal |
|---|---|---|
| Terra Classic | ~6 s | **200,000** |
| BSC | ~0.75 s (post-Maxwell — **confirm**) | **1,600,000** |
| Ethereum | 12 s | **100,800** |
| Solana | n/a — per-epoch credits do not expire | — |

## 4. Oracle — epoch, delta, and bounds per domain

- `epoch_duration_secs` = **21,600** (6h) · `max_delta_bps` = **2,000** (20%)
- Bounds: **min = current ÷ 3 · max = current × 3** — and "current" is READ FROM
  THE PRODUCTION ORACLE by the script itself AT THE MOMENT of the deploy (no fixed
  value: the doc ages, the chain does not). The tables below are only the SNAPSHOT
  of 2026-08-18 for reference/audit. A 20%/epoch delta limits the drift.

### On Terra Classic — REAL CONVENTION of cw-hyperlane (measured on-chain 08-18)

The IDA formula (official TC deploy guide): `fee_uluna = gas × gas_price_destination ×
exchange_rate / 1e12`, with `exchange_rate = (LUNC_USD / NATIVE_USD) × 1e12` —
it is the **local/remote** ratio (inverse of the canonical one!). Bounds = current ÷3/×3:

| Domain | current (rate · gas) | rate bounds | gas_price bounds |
|---|---|---|---|
| 1 (Ethereum) | 376 · 1e10 (10 gwei) | [125 · 1,128] | [3.33e9 · 3e10] wei |
| 56 (BSC) | 1,098 · 3e9 (3 gwei) | [366 · 3,294] | [1e9 · 9e9] wei |
| 1399811149 (Solana) | 383,001,553,014 · 1 (lamport model) | [1.28e11 · 1.15e12] | [1 · 10] |

### On the remotes (domain 132556 = Terra Classic) — REAL CONVENTION (measured on-chain 08-18)

⚠️ The remote IGPs are CUSTOM (`TerraClassicIGPStandalone` + `TerraClassicOracle`
on EVM; overhead-IGP on Solana) with their own calibration validated in production
(`tc-cw-hyperlane/terraclassic/doc/WARP-GAS-CONFIG.md`). The bounds anchor the
CURRENT VALUES (÷3 · ×3) — not the theoretical convention:

| Local chain | current values (rate · gas) | rate bounds | gas_price bounds |
|---|---|---|---|
| BSC | 9,047,190 · 1e10 | [3,015,730 · 27,141,570] | [3.33e9 · 3e10] |
| Ethereum | 26,585,078 · 1e10 | [8,861,692 · 79,755,234] | [3.33e9 · 3e10] |
| Solana (scale 1e19 + 10^(9−decimals)) | 2.94e10 · 28,325 · decimals 6 · overhead 3e6 | [9.8e9 · 8.82e10] | [9,442 · 84,975] |

Validated formulas: EVM `wei=(gas+overhead)×gasPrice×rate/1e10` · Solana
`lamports=(gas+overhead)×gasPrice×rate/1e19×10^(9−decimals)`.
EVM oracle: `setRemoteGasData(uint32,uint128,uint128)` FLAT (selector 0x666af432)
— GasOracleGovernor.sol uses this signature. TODO: recalibrate the EVM/SOL
formula of the oracle-agent to the per-target convention (today it computes in the canonical convention).

## 5. Operators, quorum, and delivery epoch (Solana)

- Start: **2 operators, quorum 2-of-2** (the two current agents) —
  functional, but with no tolerance for an outage; **immediate goal: 3 operators,
  quorum 2-of-3** (spec §12: open system, the 3rd joins without asking permission).
- Delivery epochs (Solana): 6h + a finality slack of **32 slots (~13s)**
  before closing the report; slot window in the report = the epoch in slots.

## 6. Multisig and ISM (remotes) — MODEL APPROVED BY GOVERNANCE

- **Approved composition**: multisig of the Hyperlane validators — **3 validators
  that validate TC + 1 signer that does NOT validate** (4 members). It is this
  multi-signature account that receives ownership of the Vault/Governor/IGP/ISM on the
  remotes at the end of the deployment (until then, owner = deployer).
- **On the threshold** (decision open within the approved model):
  - `3-of-4`: tolerates 1 absentee, BUT the 3 validators alone reach the
    threshold — the spec §12 alert (whoever validates checkpoints controlling the ISM
    has indirect access to the collateral) is only partially mitigated;
  - `4-of-4`: always requires the non-validator (neutralizes collusion), but with no
    fault tolerance — 1 lost key blocks everything;
  - Recommended evolution: add a **2nd non-validator** (3 val + 2 ext,
    threshold **4-of-5**) — validators alone cannot act AND it tolerates 1 absentee.
- ISM **3-of-4** with the 4 validators (tolerates 1 offline; forging requires 3).
- **48h** timelock to change the ISM (executable in the proposal text).

## 7. Seeding of the pools

Initial pool = **100× the fee** per network (covers the first cycle before the
IGP Sweep/claim feeds it): TC 5,000 LUNC (~$0.24) · BSC 0.005 BNB (~$3) ·
ETH 0.04 ETH (~$77) · SOL 0.3 SOL (~$23).

## 8. HANDOFF checklist (end of deployment — nothing can be left out)

Today `terra1run9wz…26mawp` is owner AND admin of everything (verified on-chain on
2026-08-18). At the end of the deployment, transfer:

### Terra Classic → governance module (`terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n` — confirmed in the official deploy guide)
- [ ] `owner` of the Mailbox, multisig ISM, IGP, and IGP-oracle (Ownable, 2 steps:
      init by the deployer + claim via proposal)
- [ ] `owner` of the relayer-reward-vault and the oracle-governor (UpdateConfig/SetOwner)
- [ ] **`admin` (migrate) of ALL contracts** → gov or `--no-admin`
      (the admin is what allows a silent migrate — do not forget!)
- [ ] ownership of the StorageGasOracle should already be on the oracle-governor (Phase 1)

### BSC / Ethereum → validators' multisig (3 TC validators + 1 non-validator — approved model; threshold: see §6)
- [ ] `owner` of the Vault and the GasOracleGovernor (2 steps: transferOwnership + acceptOwnership)
- [ ] `owner` of the IGP, the ISM, and the StorageGasOracle→governor
- [ ] proxy admin / upgrade rights of the Hyperlane contracts, if upgradeable

### Solana → multisig
- [ ] `TransferIgpOwnership` → the governor's config PDA (with a test on devnet first)
- [ ] **upgrade authority of the rrv and igp-oracle-governor programs** → multisig
- [ ] governor multisig (`SetMultisig`) pointing to the real multi-signature account


The final governance proposal = this handoff + the parameters from sections 2–7.
