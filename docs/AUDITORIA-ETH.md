# Audit Record — Ethereum Deploy (Phase 3)

**Date:** 2026-08-18 · **Chain:** Ethereum mainnet (1) · **Signer/owner:** `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae`
**Source:** `evm/src/*.sol` from this repository · solc 0.8.22 via-ir · EIP-1559 gas (~0.09 gwei at deploy)

## Deployed contracts

| Contract | Address |
|---|---|
| **RelayerRewardVault** | `0xDf90d3b7FF98466E148B334128374807b3e89EbD` |
| **GasOracleGovernor** | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` |

Vault constructor: `mailbox=0xc005dc82…D239 · owner=0xEF81…00ae ·
reward=400000000000000 (0.0004 ETH) · window=100800 blocks`.
Governor constructor: `oracle=0x3987cCE8…96cE · owner=0xEF81…00ae ·
operators=[0xEF81…00ae] · quorum=1 · epoch=21600s · delta=2000 bps`.

## Target Warp/IGP (production — see WARP-IGORFAKE.md)

| Piece | Address |
|---|---|
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (TerraClassicIGPStandalone) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle (TerraClassicOracle) | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |

## On-chain verified state (2026-08-18)

| Check | Result |
|---|---|
| `oracle.owner()` | ✅ = governor `0xa1803b36…fEFA` |
| `igp.beneficiary()` | ✅ = vault `0xDf90d3b7…9EbD` |
| `governor.isOperator(0xEF81…00ae)` / `quorum()` | ✅ true / 1 |
| `setBounds(132556)` | ✅ rate [8861692·79755234] · gas [3333333333·30000000000] · current read from the oracle at deploy (26585078·1e10) ÷3·×3 |
| `governor.currentEpoch()` | 82735 |
| Pool (vault balance) | **0** — seeding SKIPPED due to low balance. Seed: `cast send 0xDf90d3b7…9EbD --value 40000000000000000 --private-key <PK> --rpc-url https://ethereum-rpc.publicnode.com` (the value can be reduced) |

## Pending items

- [ ] Seed the pool (suggested 0.04 ETH; any value works to start).
- [ ] Handoff: vault/governor/igp/oracle/ISM → validators multisig (§8).

## Vault v2 (ClaimRemote) — 2026-08-19

New deploy `0x04096dCBbBB0FA58a312761c38E1d3B9F64631F1` (v1 `0xDf90d3b7…9EbD`
deprecated, pool 0). `igp.setBeneficiary(v2)` ✓ · attester `0xEF81…00ae` quorum 1 ·
binding dom 132556 → `terra1run9wz…` · remote reward **9,294,377,050,000 wei**
(= real fee: (50k+300k overhead) × gasPrice 1e10 × rate 26555363 / 1e10).
