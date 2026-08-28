# Migration Plan — RESULT (English translation)

> **Verified result (reproducible build, optimizer 0.17.0, compared to on-chain data_hash):**
> Only the **TC relayer-reward-vault** changed bytecode (2 production error strings translated).
> Everything else is **byte-identical to on-chain → no migration**:
>
> | Contract | on-chain data_hash | new build | migrate? |
> |---|---|---|---|
> | TC oracle-governor | `3383e2bc…` | `3383e2bc…` | ✅ identical — no |
> | **TC relayer-reward-vault** | `f3bc80e6…` | `339b8257…` | ➡️ **yes** |
> | Solana pod | sha `f2f434d8…` | `f2f434d8…` | ✅ identical — no |
> | BSC / ETH (immutable) | — | no PT prod strings | ✅ no |
>
> **To do the vault migration (supervised):** build the wasm (reproducible), then
> `KEY=<terrad-key> bash deploy/tc-migrate-vault-i18n.sh` (store → migrate `{}` → verify;
> reversible to code_id 11596). The admin is the relayer `terra1run9wz…`.

---

# Migration Plan — Full contract string translation + on-chain migration

> **Why this is a separate, supervised task.** Translating source **comments** does not change the
> compiled bytecode, so the deployed contracts still match the source `data_hash` — no migration
> needed. Translating **string literals** (error/log text) DOES change the bytecode, which breaks the
> source↔on-chain mirror unless the contracts are re-deployed/migrated. Because these are **live
> mainnet contracts holding third-party funds**, the on-chain migration must be executed **with a human
> supervising**, never unattended. This document is the runbook for that supervised session.
>
> Status: the repo is translated (docs, scripts, and contract **comments**). Contract **string
> literals** are still to be translated as part of this plan, per chain.

## Feasibility per chain (verified)

| Chain | Contracts | Upgrade model | Migration effort | Risk |
|---|---|---|---|---|
| **Solana** | pod (vault+governor, `2mQZcHYL…`) | **upgradeable** (authority `BirXd4Q…`) | in-place `solana program deploy` | low (reversible — re-upgrade with prior build) |
| **Terra Classic** | oracle-governor `terra1z7jmlky…`, vault `terra1gqkrh2…` | **migratable** (`migrate` entry point, `MigrateMsg {}` empty, state preserved) | store new wasm → `MsgMigrateContract` | low-moderate (reversible; needs migrate admin) |
| **BSC** | GasOracleGovernor, RelayerRewardVault | **IMMUTABLE** (constructor-set, no proxy) | **redeploy + re-point** | high (irreversible) |
| **Ethereum** | GasOracleGovernor, RelayerRewardVault | **IMMUTABLE** (constructor-set, no proxy) | **redeploy + re-point** | high (irreversible, expensive gas) |

## Order (do the reversible chains first, EVM last and most carefully)

### 1) Solana (reversible)
1. Translate string literals in `svm/programs/**` (governor + vault + pod).
2. `cd svm && cargo test` (all green) + `clippy -D warnings` + `cargo build-sbf`.
3. `solana program deploy svm/target/deploy/pod.so --program-id 2mQZcHYL… --upgrade-authority <BirXd4Q keypair>`
   (no `extend` if new size ≤ current allocation).
4. Verify: `solana program show` (new slot) + a real oracle-agent submission succeeds.
5. Rollback if needed: rebuild from the prior commit and re-deploy.

### 2) Terra Classic (reversible, needs migrate admin)
1. Confirm we hold the **migrate admin** of both contracts (`terrad q wasm contract <addr>` → `admin`).
   If the admin is governance/multisig, this step requires a governance/multisig tx.
2. Translate string literals in `contracts/oracle-governor/**` + `contracts/relayer-reward-vault/**`.
3. Build reproducibly (optimizer 0.17.0) → new `data_hash`/checksum.
4. `MsgStoreCode` (new wasm) → new `code_id`; then `MsgMigrateContract` (admin) with `{}`.
5. Verify: `code_id` updated, `LayoutCheck`/`solvency` queries still answer, no state loss.
6. Rollback: migrate back to the previous `code_id`.

### 3) BSC, then Ethereum (IMMUTABLE — highest care)
> Immutable contracts cannot be upgraded. "Migration" = deploy NEW instances and re-point the system.
> This changes canonical addresses and **must** be reflected in the hyperlane-registry.
1. Fund the deployer wallet with enough **BNB / ETH** for deployment gas.
2. Translate string literals in `evm/src/*.sol`; `cd evm && forge test` (all green, `via_ir=true`).
3. Deploy the NEW `GasOracleGovernor` + `RelayerRewardVault` (record new addresses).
4. **Re-point** everything to the new contracts (multisig where required):
   - `StorageGasOracle.transferOwnership(newGovernor)` (OZ, single step — verify address 3×).
   - `IGP.setBeneficiary(newVault)`.
   - `governor.setBounds(domain, …)` per domain on the new governor.
   - migrate/seed any pool balance from the old vault if applicable.
5. Update **hyperlane-registry** with the new addresses; announce the change.
6. Verify end-to-end: a low-value delivery → reward payable from the new vault + commission path.
7. **No rollback** — the old contracts remain; only forward re-pointing. Double-check every address.

## Pre-flight checklist (every chain)
- [ ] Full test suite green (91 tests: `cargo test` ×2 + `forge test`) + `clippy -D warnings`.
- [ ] Reproducible build; new checksum/`data_hash` recorded and reviewed.
- [ ] Admin/authority/keys confirmed and **funded**.
- [ ] A human is watching; a rollback (or forward-fix) path is written down.
- [ ] Monitoring (`monitor-web`) open during and after.

## Recommendation
The **comments-only** translation already makes the contract source readable in English with **zero**
on-chain risk and keeps the audited source↔on-chain mirror intact. Translating the remaining short
error/log strings is **low value**; do it only alongside a genuinely needed upgrade, and never on the
immutable EVM contracts just for text. If you still want the full string translation + migration, run
this plan **supervised**, reversible chains (Solana, TC) first, EVM last.
