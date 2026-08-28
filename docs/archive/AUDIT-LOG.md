# Consolidated Audit Record — proof-of-delivery (4 networks)

**Snapshot:** 08/18/2026 · system ACTIVE on the 4 networks. Details per network:
`AUDIT-TC.md` · `AUDIT-BSC.md` · `AUDIT-ETH.md` · `AUDIT-SOLANA.md`.
Warp/validators: `WARP-IGORFAKE.md`. How to operate each contract: `CONTRACT-OPERATION.md`.

## 1. Terra Classic (domain 132556) — COLLATERAL side

### Our contracts (source: `contracts/`, reproducible build optimizer 0.17.0)

| Contract | Address | code_id | SHA-256 (= on-chain data_hash) |
|---|---|---|---|
| **relayer-reward-vault v2** (ClaimRemote) | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` | **11589** | `e24a5e66ab4a503c6acf369710b717310362d2ae5fa7b9800542c8272b2fc801` |
| **oracle-governor** | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` | 11587 | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` |

### Hyperlane infra (pre-existing, verified byte by byte in Phase 0)

| Piece | Address |
|---|---|
| Mailbox (code_id 11371) | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` |
| IGP (beneficiary = **vault** ✓) | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP oracle (owner = **governor** ✓) | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` |
| Warp IGORFAKE (collateral) | `terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel` |
| cw20 token | `terra1lpkaaqjaq8zfwktge3vy0zg46nxxsynsge2wxa7addpweu2w6gmsy3lhkr` |
| Governance module (handoff target) | `terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n` |

### Current roles and parameters

- **Owner (vault + governor):** `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`
  (deployer/relayer — TEMPORARY until the handoff → governance).
- **Price operator:** `terra1run9wz…26mawp` · quorum **1**.
- Fee 50 LUNC/local delivery · **v2 ClaimRemote**: 33 LUNC/remote delivery
  (bindings: SOL `PbEo7Fn2…` · BSC `0x8f08…5291` · ETH `0xef81…00ae`; attester
  `terra1run9wz…`, quorum 1) · epoch 6 h · delta 20%.
- INBOUND ISMs (official Hyperlane validators): ETH **6-of-9** · BSC **4-of-6** · SOL **3-of-5**.

## 2. BSC (domain 56) — synthetic

| Piece | Address |
|---|---|
| **RelayerRewardVault v2** (ClaimRemote) | `0x1A41144ccbA0797BB0e9e448Aa3C330Eb68347D1` (v1 `0x8b3A9eEB…145D` deprecated) |
| **GasOracleGovernor** | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` |
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (beneficiary = **vault v2** ✓) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (owner = **governor** ✓) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |
| Warp IGORFAKE | `0x3605D8946FC6F5A75d89d92173100F59743B5318` |
| ISM (threshold 1) | `0xa82087B8eea0394B1476f716B91c10531025Ef42` |
| ISM validator | `0x71B2B8C36a0C76b74Be92eb7915E26A69b3B03eB` |

- **Owner (vault+governor) and sole operator (quorum 1):** `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` (= BSC relayer).
- Fee 0.00005 BNB · window 1,600,000 blocks · bounds(132556): rate `[3015730·27141570]` · gas `[3333333333·30000000000]`.
- ⚠️ Pool: **0** (seed pending).

## 3. Ethereum (domain 1) — synthetic

| Piece | Address |
|---|---|
| **RelayerRewardVault v2** (ClaimRemote) | `0x04096dCBbBB0FA58a312761c38E1d3B9F64631F1` (v1 `0xDf90d3b7…9EbD` deprecated) |
| **GasOracleGovernor** | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` |
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (beneficiary = **vault v2** ✓) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle (owner = **governor** ✓) | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |
| Warp IGORFAKE | `0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5` |
| ISM (threshold 1, same validator as BSC) | `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b` |

- **Owner and sole operator (quorum 1):** `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` (= ETH relayer).
- Fee 0.0004 ETH · window 100,800 blocks · bounds(132556): rate `[8861692·79755234]` · gas `[3333333333·30000000000]`.
- ⚠️ Pool: **0** (seed pending).

## 4. Solana (domain 1399811149) — synthetic

| Piece | Address |
|---|---|
| **pod program** (vault+governor MERGED; 1st byte routes 0=vault/1=governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` |
| rrv config PDA (**the POOL**; IGP beneficiary ✓) | `Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w` |
| gov config PDA (**IGP owner** ✓) | `4sZAfqDqEmR7LMWjrdNmoEkv8S6BDdnDkh5mfADenaaA` |
| IGP program | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` |
| IGP account inner (receives payments) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| Overhead IGP (referenced by the warp) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| Mailbox | `E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi` |
| Warp IGORFAKE | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` |
| Synthetic mint | `CeLHx5Wm9AzuWRnP4URMfNqNa9kDDrnsNGoATCS96QwD` |
| ISM program | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` |

- **pod upgrade authority + governor multisig:** `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (deployer).
- **Operators:** `BirXd4Q…` and `PbEo7Fn2…` (relayer, registered 08/18) · **quorum 1**.
- Fee 0.003 SOL local · **v2 ClaimRemote**: 499,000 lamports/remote delivery (epoch report; binding PbEo→terra1run9wz) · epoch 21,600 s · vault quorum **1** (2→1 via 2-of-2 proposal, 08/18) · bounds(132556): rate `[9800000000·88200000000]` ·
  gas `[9441·84975]` · decimals 6 · pool **0.3 SOL** ✓.
- `pod.so` 184,904 bytes, deploy `--max-len` exact (cost 1.359 SOL + finalize; rent recoverable).

## 4b. ClaimRemote (v2) — origin fee on the 4 chains

| Origin | Mechanism | Reward/delivery | Executor binding |
|---|---|---|---|
| TC (code_id 11589) | per id | 33 LUNC | SOL `PbEo…` · BSC `0x8f08…` · ETH `0xEF81…` |
| BSC (`0x1A41144c…`) | per id | 2,259,538,750,000 wei | dom 132556 → `terra1run9wz…` |
| ETH (`0x04096dCB…`) | per id | 9,294,377,050,000 wei | dom 132556 → `terra1run9wz…` |
| Solana (pod, upgrade 08/19) | per epoch | 499,000 lamports | dom 132556 → `terra1run9wz…` (PDAs `8N3sq5Xg…`/`GTeqFxoQ…`) |

Attesters = owner of each chain · quorum 1 (test). **Economy (08/19): the
per-delivery payment is UNIQUE, at the origin** — destination rewards reduced to
1 symbolic unit (TC tx `965542B7…` · BSC `0x2ce94a7a…` · ETH `0xa0d516d3…`
· SOL `5Q7C34EP…`). Expansion: `EXPANSION-MANUAL.md`.

## 4c. Trustless receipt (TC↔BSC proven 08/19) — DEFINITIVE model

Vaults with `send_receipt`/`handle`: TC `terra1gqkrh2…` (code_id 11592) · BSC
`0x34E06a7793877EC5251b1dC230aD7cD577d231f4` (ism=`0xa82087B8…`, the warp ISM;
since 08/20/2026 both use the mutable ISM `0xF6b0cDD3…` — see `ISM-VALIDATORS.md`).
Mutual router + operator 0 to/from. TRUSTLESS payment: the destination proves the
delivery, the receipt is validated by the bridge validators, the origin pays.
Details: `TRUSTLESS-RECEIPT.md`. Previous attestation vaults (BSC 0x1A41144c,
0xAe95a3) deprecated. ETH/Solana pending replication.

## 5. Relayer in operation (1 operator in this phase)

| Chain | Relayer address |
|---|---|
| Terra Classic | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |
| BSC | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |
| Ethereum | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |
| Solana | `PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS` |

## 6. CURRENT centralization state and handoff target

Today (deployment phase): the deployer of each network accumulates owner + operator +
relayer. **Target approved by governance**: TC → governance module;
BSC/ETH/Solana → validators multisig (3 TC validators + 1 non-validator;
threshold to be defined — see §8 of `PROPOSAL-PARAMETERS.md`). Step-by-step
procedures in `CONTRACT-OPERATION.md` §5.

## 7. Quick verification (one line per invariant)

```bash
NODE=https://rpc.terra-classic.hexxagon.io
# TC: oracle owner = governor · IGP beneficiary = vault · solvency
terrad q wasm contract-state smart terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d '{"ownable":{"get_owner":{}}}' --node $NODE
terrad q wasm contract-state smart terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz '{"igp":{"beneficiary":{}}}' --node $NODE
terrad q wasm contract-state smart terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q '{"solvency":{}}' --node $NODE
# BSC / ETH: oracle owner = governor · IGP beneficiary = vault
cast call --rpc-url https://bsc-dataseed.bnbchain.org 0x7dE950f8F0a037783989a6BE84B3620916552306 "owner()(address)"
cast call --rpc-url https://bsc-dataseed.bnbchain.org 0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923 "beneficiary()(address)"
cast call --rpc-url https://ethereum-rpc.publicnode.com 0x3987cCE8f08037EBF93Ef3a934753540A94196cE "owner()(address)"
cast call --rpc-url https://ethereum-rpc.publicnode.com 0x9650F1f8DB492750323172145e67Df4e89E964Aa "beneficiary()(address)"
# Solana: owner/beneficiary embedded in the Igp account (FPTvDso…) — offsets 43/75
# (verification script: deploy/solana-init.mjs reads and validates; see AUDIT-SOLANA.md)
solana balance Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w -u https://api.mainnet-beta.solana.com
```

## 8. Reproducibility of the binaries

- CosmWasm: `cosmwasm/optimizer:0.17.0` over this repo → sha256 identical to the
  on-chain data_hash (11587/11588). `artifacts/checksums.txt`.
- EVM: solc 0.8.22 `via_ir` (versioned foundry.toml) — `forge build` + compare
  `cast code` with the deployed bytecode.
- Solana: `cargo build-sbf` (pinned workspace, versioned Cargo.lock,
  opt-level=z) → `pod.so` 184,904 bytes.
