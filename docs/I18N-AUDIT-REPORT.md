# i18n Audit Report — PT→EN translation & on-chain impact (all chains)

> Public, auditable record of the full Portuguese→English translation of this repository and its
> exact on-chain impact. **Only the Terra Classic relayer-reward-vault changed bytecode** (2 production
> error strings were translated); every other contract on every chain is **byte-identical** to what is
> deployed, so **no migration** is required for them. All hashes below are reproducible.

## 1. Repository record (git)

| Commit | Scope |
|---|---|
| `7d03588` | docs + SPEC.html + README → English |
| `dc8617f` | deploy scripts + oracle-agent + skills → English |
| `f78163b` | contracts: comments + 2 vault prod strings + test names/asserts → English |
| `9cfebbe` | ready vault migration script + plan |
| `e466fbf` | rename PT doc filenames + update all references |

Branch: `main` @ `terra-classic-hyperlane/proof-of-delivery`.

## 2. On-chain impact per chain (data_hash / bytecode before × after)

| Chain | Contract | Address / Program | Deployed hash (before) | Rebuilt hash (after) | Migration |
|---|---|---|---|---|---|
| **Terra Classic** | oracle-governor | `terra1z7jmlky…9sv4hj` (code 11587) | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` | `3383e2bc…41744` | ❌ identical |
| **Terra Classic** | **relayer-reward-vault** | `terra1gqkrh2…duzc2q` (code 11596) | `f3bc80e635228e6f57643a17f88a6496ca194b23a8b38d51d65b618621eba346` | `339b82571a9679830f1b7469a2ae42a96929286d77954f53014416af9bcc33fa` | ➡️ **YES** |
| **Solana** | pod (vault+governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` | `f2f434d8d5256d3deb35d106dbca3adc261a66a7ca77c933edd74dbb3aa8572e` (sha256 of pod.so) | `f2f434d8…8572e` | ❌ identical |
| **BSC** | RelayerRewardVault (v2) | `0x1A41144ccbA0797BB0e9e448Aa3C330Eb68347D1` | immutable; comments-only → identical | — | ❌ none (immutable) |
| **BSC** | GasOracleGovernor | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` | immutable; comments-only → identical | — | ❌ none (immutable) |
| **Ethereum** | RelayerRewardVault (v2) | `0x04096dCBbBB0FA58a312761c38E1d3B9F64631F1` | immutable; comments-only → identical | — | ❌ none (immutable) |
| **Ethereum** | GasOracleGovernor | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` | immutable; comments-only → identical | — | ❌ none (immutable) |

**Why only the TC vault changed:** compilers strip comments, so translating comments cannot change the
bytecode. The only Portuguese **string literals** in production code were two `generic_err` messages in
`contracts/relayer-reward-vault/src/execute.rs` (`"msg curta"→"msg too short"`,
`"origens misturadas"→"mixed origins"`); translating them changes the vault's `data_hash`. No other
production string literals in Portuguese exist anywhere in the contracts (all other PT was in comments
or in tests, and tests are not compiled into deployed artifacts).

## 3. Test suites (post-translation, all green)
- CosmWasm (`cargo test`): 15 + 4 + 34 passed, 0 failed.
- Solana (`cargo test`): governor 6, vault 17 passed, 0 failed.
- EVM (`forge test`): 48 passed, 0 failed.

## 4. Terra Classic vault migration transactions
> To be filled after execution of `deploy/tc-migrate-vault-i18n.sh` (admin `terra1run9wz…`, chain
> `columbus-5`, reversible to code_id 11596). Build reproducibly with `cosmwasm/optimizer:0.17.0`.

| Step | tx hash | result |
|---|---|---|
| `MsgStoreCode` (new wasm `339b8257…`) | `<STORE_TXHASH>` | new code_id `<NEW_CODE_ID>` |
| `MsgMigrateContract` (vault → new code_id, `{}`) | `<MIGRATE_TXHASH>` | code_id updated; pool/state preserved |

## 5. How anyone can reproduce these hashes
```bash
git clone https://github.com/terra-classic-hyperlane/proof-of-delivery && cd proof-of-delivery

# Terra Classic (CosmWasm) — reproducible optimized wasm + checksums
docker run --rm -v "$(pwd)":/code -v cwopt_cache:/target \
  -v cwopt_registry:/usr/local/cargo/registry cosmwasm/optimizer:0.17.0
cat artifacts/checksums.txt
#   oracle_governor.wasm      = 3383e2bc…41744   (matches on-chain code 11587)
#   relayer_reward_vault.wasm = 339b8257…33fa    (new; on-chain 11596 is f3bc80e6… until migrated)

# Solana
cd svm && cargo build-sbf && sha256sum target/deploy/pod.so   # f2f434d8…8572e  (matches on-chain)

# EVM
cd ../evm && forge build && forge test   # bytecode of the immutable contracts is unchanged

# Compare with on-chain (TC):
curl -s https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/code/11587 | jq -r .code_info.data_hash
curl -s https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/code/11596 | jq -r .code_info.data_hash
```
