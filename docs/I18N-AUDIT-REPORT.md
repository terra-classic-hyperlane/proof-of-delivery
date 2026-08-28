# i18n Audit Report — PT→EN translation & on-chain impact (all chains) ✅ COMPLETE

> Public, auditable record of the full Portuguese→English translation of this repository and its
> exact on-chain impact. **Status: COMPLETE (2026-08-28).** The repository — docs, scripts, contract
> source and tests — contains **zero Portuguese**. **Only the Terra Classic relayer-reward-vault
> changed bytecode** (2 production error strings were translated) and it **was migrated** (§4);
> every other contract on every chain is byte-identical in executable code to what is deployed, so
> no migration is required for them (§2a/§2b). The production VPS was redeployed from `main` — after
> merging a VPS-only hotfix back into the repo (§6) — so repo, on-chain and production are fully
> consistent. All hashes below are reproducible.
>
> **The rule this report demonstrates:** the on-chain `data_hash` covers the **compiled bytecode**,
> not the source text. Compilers strip comments, and tests are never deployed — so translating
> comments, doc-comments or test code **cannot** change the hash. Only changes to **executable
> code** — logic, values, or runtime **string literals** (e.g. error messages) — change the bytecode
> and require a migration. That is exactly why the TC vault (2 error strings) migrated and nothing
> else did.

## 1. Repository record (git)

| Commit | Scope |
|---|---|
| `7d03588` | docs + SPEC.html + README → English |
| `dc8617f` | deploy scripts + oracle-agent + skills → English |
| `f78163b` | contracts: comments + 2 vault prod strings + test names/asserts → English |
| `9cfebbe` | ready vault migration script + plan |
| `e466fbf` | rename PT doc filenames + update all references |
| `518c09e` | record executed TC vault migration (code_id 11635, txs + on-chain hash proof) |
| `2b77e7c` | byte-level proof no Solana/EVM migration is needed (on-chain dump + metadata-trailer analysis) |
| `81ad764` | final sweep: last 2 PT remnants (EVM test fn name + SVM doc comment) — repo now 100% English |
| `3ad972c` | merge VPS-only hotfix into main: single price-round PDA per domain in oracle-agent `solana.js` (§6) |

Post-`81ad764` verification: PT-scan over all `.rs`/`.sol` (src + tests) returns **zero matches**;
`forge test` 48/48 green; `pod.so` rebuilt after the doc-comment edit hashes to the **same**
`f2f434d8…8572e` as the live mainnet program — a live demonstration that comment edits do not
change bytecode.

Branch: `main` @ `terra-classic-hyperlane/proof-of-delivery`.

## 2. On-chain impact per chain (data_hash / bytecode before × after)

| Chain | Contract | Address / Program | Deployed hash (before) | Rebuilt hash (after) | Migration |
|---|---|---|---|---|---|
| **Terra Classic** | oracle-governor | `terra1z7jmlky…9sv4hj` (code 11587) | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` | `3383e2bc…41744` | ❌ identical |
| **Terra Classic** | **relayer-reward-vault** | `terra1gqkrh2…duzc2q` (code 11596 → **11635**) | `f3bc80e635228e6f57643a17f88a6496ca194b23a8b38d51d65b618621eba346` | `339b82571a9679830f1b7469a2ae42a96929286d77954f53014416af9bcc33fa` | ✅ **MIGRATED** (§4) |
| **Solana** | pod (vault+governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` | `f2f434d8d5256d3deb35d106dbca3adc261a66a7ca77c933edd74dbb3aa8572e` (sha256 of pod.so) | `f2f434d8…8572e` | ❌ **byte-identical, proven vs on-chain dump** (§2a) |
| **BSC** | RelayerRewardVault (receipt, active) | `0x34E06a7793877EC5251b1dC230aD7cD577d231f4` | 10417 B, PT-source metadata fingerprint | executable body identical; only 53-byte metadata trailer differs (§2b) | ❌ none (immutable; nothing executable changed) |
| **BSC** | GasOracleGovernor | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` | 6372 B, PT-source metadata fingerprint | executable body identical; only 53-byte metadata trailer differs (§2b) | ❌ none (immutable; nothing executable changed) |
| **Ethereum** | RelayerRewardVault (v2 attestation, active) | `0x04096dCBbBB0FA58a312761c38E1d3B9F64631F1` | 5892 B (deployed from the pre-receipt v2 source revision) | translation changed nothing executable (§2b) | ❌ none (immutable) |
| **Ethereum** | GasOracleGovernor | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` | 6372 B, PT-source metadata fingerprint | executable body identical; only 53-byte metadata trailer differs (§2b) | ❌ none (immutable; nothing executable changed) |

### 2a. Solana — byte-level proof against the live program (2026-08-28)
`solana program dump 2mQZcHYL…ZUFj` from mainnet-beta returned bytecode whose first 239,464 bytes
hash to `f2f434d8…8572e` — **exactly** the sha256 of `pod.so` built from the translated source —
and every byte past that is zero padding. The translation did not change a single byte of the
deployed program. An upgrade would be a paid no-op; none is needed.

### 2b. EVM — why translated comments cannot require a redeploy
`solc` appends a 53-byte CBOR **metadata trailer** to every contract's bytecode: an IPFS hash that
fingerprints the *source text* (comments included). It is data after the final `INVALID` opcode —
it can never execute. Measured comparison of the build from the last pre-translation commit
(`dc8617f`) vs the translated source (both solc 0.8.22, via-IR, runs=200):
- `RelayerRewardVault`: 10,417 bytes — **first 10,364 bytes (all executable code) byte-identical**; only the trailer differs.
- `GasOracleGovernor`: 6,372 bytes — **first 6,319 bytes byte-identical**; only the trailer differs.

On-chain verification (eth_getCode, 2026-08-28): the BSC active vault `0x34E06a77…` and both
governors match the PT-source build **exactly** — same size, same metadata trailer, differing only
in the immutable-variable slots the constructor filled (expected). The deployed contracts therefore
carry the original PT-source fingerprint; the repository's translated source compiles to the same
executable code. Since these contracts are **immutable** (no migrate/upgrade path), the only way to
change the trailer would be redeploying at new addresses and re-pointing IGP/ISM/registry — real
risk for a non-executable fingerprint. **No redeploy is warranted or planned.**
Note: the ETH vault `0x04096dCB…` (5,892 B) was deployed from the earlier v2 attestation source
revision (commit `28be74f`, before quote/receipt/setIsm were added) — a pre-existing version gap
unrelated to the translation.

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

## 4. Terra Classic vault migration transactions ✅ EXECUTED (2026-08-28)
> Executed via `deploy/tc-migrate-vault-i18n.sh` (admin `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`,
> chain `columbus-5`). Reversible to code_id 11596. Built reproducibly with `cosmwasm/optimizer:0.17.0`.

| Step | tx hash | result |
|---|---|---|
| `MsgStoreCode` (new wasm `339b8257…33fa`) | `0DF2F74B228F28CD80E7C8EE1E828E40BC4AA90F1406C6C667D0831474F492E9` | new **code_id 11635** |
| `MsgMigrateContract` (vault → 11635, `{}`) | `0472A13D3950A6648950B591CA2D3BCB6D6408B335481159A730B9DF5E1CDC0A` | migrated; pool/state fully preserved |

Post-migration verification (on-chain):
- `code/11635` → `data_hash = 339B82571A9679830F1B7469A2AE42A96929286D77954F53014416AF9BCC33FA` — **exactly matches** the reproducible build.
- Vault `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` → `code_id: 11635`, admin unchanged.
- State preserved: `pool = 13489469826 uluna`, `claims_payable = 13489469826` (solvent, `claims_payable == pool`), `reward_per_delivery = 1`.

## 5. How anyone can reproduce these hashes
```bash
git clone https://github.com/terra-classic-hyperlane/proof-of-delivery && cd proof-of-delivery

# Terra Classic (CosmWasm) — reproducible optimized wasm + checksums
docker run --rm -v "$(pwd)":/code -v cwopt_cache:/target \
  -v cwopt_registry:/usr/local/cargo/registry cosmwasm/optimizer:0.17.0
cat artifacts/checksums.txt
#   oracle_governor.wasm      = 3383e2bc…41744   (matches on-chain code 11587)
#   relayer_reward_vault.wasm = 339b8257…33fa    (matches on-chain code 11635 — the vault's current code)

# Solana
cd svm && cargo build-sbf && sha256sum target/deploy/pod.so   # f2f434d8…8572e  (matches on-chain)

# EVM
cd ../evm && forge build && forge test   # bytecode of the immutable contracts is unchanged

# Compare with on-chain (TC):
curl -s https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/code/11587 | jq -r .code_info.data_hash   # 3383e2bc… (governor)
curl -s https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/code/11635 | jq -r .code_info.data_hash   # 339b8257… (vault, current)
curl -s https://lcd.terra-classic.hexxagon.io/cosmwasm/wasm/v1/code/11596 | jq -r .code_info.data_hash   # f3bc80e6… (vault, pre-migration)
```

## 6. VPS redeploy of the English scripts + hotfix merge ✅ EXECUTED (2026-08-28)

The production VPS ran the pre-translation (PT) copies of the operational scripts. They were
redeployed from `main` so that production and repository match.

**Hotfix discovered and merged into `main` first (commit `3ad972c`).** The pre-deploy checksum
comparison (VPS files vs the pre-translation repo revision) revealed that the VPS copy of
`oracle-agent/src/chains/solana.js` carried a local hotfix that was never committed: the price-round
PDA seed is `["gov","-","price","-",domain_le]` — **one round account per domain, no epoch** — the
JS companion of the Solana rent-leak fix (program commit `3c9d8e6`), matching the on-chain program's
`SEED_PRICE` derivation. `main` still derived the old per-epoch PDA; deploying it unmerged would
have reverted the fix and broken Solana submissions. The hotfix was merged into `main` **before**
the redeploy. Every other VPS file was checksum-identical to its pre-translation repo version
(i.e. no other undocumented local changes existed).

**Redeploy procedure (auditable):**
1. Full backup of the PT copies at `/root/backup-pt-20260828` on the VPS (rollback point).
2. Files updated from `main`: `oracle-agent/src/{index,claims,prices}.js`,
   `oracle-agent/src/chains/{evm,solana,terraclassic}.js`, and
   `claim-agent/{claim-agent-receipt,deliver-receipts-tc,solana-epoch-reporter}.mjs`.
3. Untouched: `.env`/`rpc.env`, `config.json`, `state.json`, and the legacy `process-*.mjs`
   (not in the repo, not referenced by any systemd unit).
4. `node --check` on the VPS for every updated file, then sequential service restarts.

**Post-deploy verification:** all 5 services active with `NRestarts=0`
(`hyperlane-validator`, `hyperlane-relayer`, `oracle-agent`, `claim-agent`, `epoch-reporter`);
logs switched from PT to EN across a single restart (claim-agent 17:13 UTC "recibos … pendentes" →
17:16 UTC "BSC→TC receipts pending delivery on TC: 0"); oracle-agent completed a full cycle
("stable (<300bps) — no submission", next round scheduled 14400 s); epoch-reporter verified its
operator and found the epoch already reported. The only outstanding issue is the pre-existing
ETH gas-delta bound on the TC governor (unrelated to translation or redeploy).

With this step, **repository, on-chain contracts and production VPS are fully consistent in
English**: the repo is the source of truth and production runs exactly what `main` contains.
