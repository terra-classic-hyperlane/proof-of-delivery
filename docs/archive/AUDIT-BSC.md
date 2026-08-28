# Audit Record — BSC Deploy (Phase 3)

**Date:** 2026-08-18 · **Chain:** BSC mainnet (56) · **Signer/owner:** `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291`
**Source:** `evm/src/*.sol` from this repository · solc 0.8.22 via-ir · RPC `bsc-dataseed.bnbchain.org`

## Deployed contracts

| Contract | Address |
|---|---|
| **RelayerRewardVault** | `0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D` |
| **GasOracleGovernor** | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` |

Vault constructor: `mailbox=0x2971b9Ae…e8a4 · owner=0x8f08…5291 ·
reward=50000000000000 (0.00005 BNB) · window=1600000 blocks`.
Governor constructor: `oracle=0x7dE950f8…2306 · owner=0x8f08…5291 ·
operators=[0x8f08…5291] · quorum=1 · epoch=21600s · delta=2000 bps`.

## Target Warp/IGP (production — see WARP-IGORFAKE.md)

| Piece | Address |
|---|---|
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |

## On-chain verified state (2026-08-18)

| Check | Result |
|---|---|
| `oracle.owner()` | ✅ = governor `0x5CF7A3a7…13E5` (ownership transferred) |
| `igp.beneficiary()` | ✅ = vault `0x8b3A9eEB…145D` |
| `governor.oracle()` | ✅ = `0x7dE950f8…2306` |
| `governor.isOperator(0x8f08…5291)` | ✅ true |
| `governor.quorum()` | 1 |
| `governor.currentEpoch()` | 82735 (6h per epoch) |
| `setBounds(132556)` | ✅ rate [3015730·27141570] · gas [3333333333·30000000000] · current read from the oracle at deploy (9047190 · 1e10) ÷3·×3 |
| `vault.owner()` | `0x8f08…5291` (deployer — handoff to multisig in §8) |
| Pool (vault balance) | **0** — seeding SKIPPED due to low balance. Seed: `cast send --legacy 0x8b3A9eEB…145D --value 5000000000000000 --private-key <PK> --rpc-url https://bsc-dataseed.bnbchain.org` |

## Pending items

- [ ] Seed the pool (0.005 BNB) once there is balance — without it `claim` reverts with `InsufficientPool`.
- [ ] Handoff: `vault`/`governor`/`igp`/`oracle`/`ISM` → validators multisig (§8 of PROPOSAL-PARAMETERS.md).

## How to audit

```bash
RPC=https://bsc-dataseed.bnbchain.org
cast call --rpc-url $RPC 0x7dE950f8F0a037783989a6BE84B3620916552306 "owner()(address)"        # = governor
cast call --rpc-url $RPC 0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923 "beneficiary()(address)"  # = vault
cast call --rpc-url $RPC 0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5 "bounds(uint32)(uint128,uint128,uint128,uint128,bool)" 132556
cast code --rpc-url $RPC 0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D   # vault bytecode (compare with forge build)
```

## Vault v2 (ClaimRemote) — 2026-08-19

EVM vaults are not migratable → **new deploy** `0x1A41144ccbA0797BB0e9e448Aa3C330Eb68347D1`
(v1 `0x8b3A9eEB…145D` deprecated, pool 0). `igp.setBeneficiary(v2)` ✓ ·
attester `0x8f08…5291` quorum 1 · binding dom 132556 → `terra1run9wz…` ·
remote reward **2,259,538,750,000 wei** (= real fee: (50k+200k overhead) ×
gasPrice 1e10 × rate 9038155 / 1e10). Flow: message dispatched FROM BSC + delivered
on TC by the operator → claim-agent attests here → BNB fee returns to the operator.
