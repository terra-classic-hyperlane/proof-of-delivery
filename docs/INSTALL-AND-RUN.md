# tc-proof-of-delivery — Installation and Execution

Technical documentation of the spec's 7 artifacts (`SPEC.html`): how to prepare the
environment, compile, test, and deploy each layer. The architecture decisions
are in the spec; the mainnet verification (Phase 0 partial) is in the `README.md`.

---

## 1. Repository layout

```
tc-proof-of-delivery/
├── SPEC.html                     # v3 specification (source of truth for the design)
├── README.md                     # code/mainnet verification + test scoreboard
├── Cargo.toml                    # COSMWASM workspace
├── contracts/
│   ├── relayer-reward-vault/     # TC vault: proof via raw query on the Mailbox
│   └── oracle-governor/          # quorum+median+bounds over the StorageGasOracle
├── evm/                          # FOUNDRY project (BSC/Ethereum)
│   ├── src/RelayerRewardVault.sol
│   ├── src/GasOracleGovernor.sol
│   └── test/
├── svm/                          # SOLANA workspace
│   └── programs/
│       ├── relayer-reward-vault/ # crate "rrv": pool on the PDA, epochs, proposals
│       ├── igp-oracle-governor/  # two doors over the IGP
│       └── mock-igp/             # TESTS ONLY: mirror of the real IGP wire-format
└── oracle-agent/                 # Node: multi-chain price feed for the governors
```

Each layer has its OWN toolchain and workspace — do not mix them (the CosmWasm and
Solana ones have mutually incompatible dependency trees).

---

## 2. Prerequisites

| Tool | Tested version | Use |
|---|---|---|
| Rust + cargo | 1.84.0 | CosmWasm and Solana (⚠️ see the pins note below) |
| `wasm32-unknown-unknown` target | — | CosmWasm build (`rustup target add wasm32-unknown-unknown`) |
| Foundry (forge) | 1.5.0 | EVM (build + tests) |
| Solana CLI + cargo-build-sbf | 4.0.0 / platform-tools 1.53 | BPF build + Solana deploy |
| Node.js | 20.x | oracle-agent |
| Docker | any | reproducible CosmWasm build (`cosmwasm/optimizer`) |

> **Note on the `Cargo.lock` files:** with rustc 1.84, several recent transitive
> dependencies require `edition2024` (rustc ≥1.85). BOTH lockfiles
> (the root `Cargo.lock` and `svm/Cargo.lock`) are already pinned to compatible
> versions — **commit them and do not run `cargo update` without need**. If
> you need to update something, update it surgically
> (`cargo update <crate>@<ver> --precise <compatible-ver>`).

---

## 3. Build and tests per layer

### 3.1 CosmWasm (Terra Classic)

```bash
cd tc-proof-of-delivery
cargo test                                     # 39 tests (unit + cw-multi-test)
cargo clippy --all-targets -- -D warnings      # clean
cargo build --release --target wasm32-unknown-unknown --lib   # development wasm
```

**PRODUCTION build (reproducible — mandatory before storing on the chain):**

```bash
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.0
# artifacts in ./artifacts/*.wasm + checksums.txt (it is the code's data_hash on the chain)
```

### 3.2 EVM (BSC / Ethereum)

```bash
cd evm
# 1st time after the clone (forge-std is not versioned):
git clone --depth 1 https://github.com/foundry-rs/forge-std lib/forge-std
forge test            # 32 tests
forge build --sizes   # Vault ~2.5 KB · Governor ~6.5 KB (via_ir enabled)
```

### 3.3 Solana

```bash
cd svm
cargo test            # 15 functional tests (solana-program-test, native execution)
cargo clippy --all-targets -- -D warnings
cargo build-sbf       # generates target/deploy/{rrv,igp_oracle_governor}.so
```

The `mock-igp` is a test artifact — it **never** goes to mainnet.

### 3.4 oracle-agent

```bash
cd oracle-agent
npm install
npm test              # exchange_rate/scales math
npm run dry-run       # full round WITHOUT signing (CoinGecko + real RPCs)
```

---

## 4. Deployment (order from spec §13 — executable summary)

> **PHASE 0 (mandatory before everything):** partially done — the raw query of
> `DELIVERIES` was validated ON MAINNET (see README). What is missing is comparing the
> code_id 11371's `data_hash` with the `checksums.txt` of the reproducible build of
> `tc-cw-hyperlane` in production.

### 4.1 Terra Classic (Phases 1–2)

```bash
# 1. store + instantiate the oracle-governor
terrad tx wasm store artifacts/oracle_governor.wasm --from operator ...
terrad tx wasm instantiate <code_id> '{
  "owner": "<GOV_MODULE_ADDRESS>",
  "oracle": "<hpl-igp-oracle>",
  "operators": ["terra1...","terra1..."],
  "quorum": 2,
  "epoch_duration_secs": 21600,
  "max_delta_bps": 2000
}' --label "oracle-governor" ...

# 2. oracle ownership → governor (2 steps):
#    a) governance executes on the oracle: {"ownership":{"init_ownership_transfer":{"next_owner":"<governor>"}}}
#    b) anyone executes on the governor: {"claim_oracle_ownership":{}}

# 3. governance sets the bounds PER DOMAIN on the governor:
#    {"set_bounds":{"domain":56,"bounds":{...}}}

# 4. store + instantiate the relayer-reward-vault
terrad tx wasm instantiate <code_id> '{
  "owner": "<gov>", "mailbox": "terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9",
  "igp": "<hpl-igp>", "denom": "uluna",
  "reward_per_delivery": "<e.g.: 1000000>",
  "claim_window_blocks": <e.g.: 100000>
}' ...

# 5. governance: IGP {"set_beneficiary":{"beneficiary":"<vault>"}}
# 6. seed the pool (BankSend of uluna to the vault) and monitor {"layout_check":{...}}
```

### 4.2 BSC / Ethereum (Phase 3)

```bash
cd evm
forge create src/RelayerRewardVault.sol:RelayerRewardVault \
  --constructor-args <MAILBOX> <MULTISIG> <REWARD_WEI> <WINDOW_BLOCKS> ...
forge create src/GasOracleGovernor.sol:GasOracleGovernor \
  --constructor-args <STORAGE_GAS_ORACLE> <MULTISIG> '[<op1>,<op2>]' 2 21600 2000 ...

# multisig:
#   governor.setBounds(domain, {...})
#   StorageGasOracle.transferOwnership(governor)     # OZ, single step
#   IGP.setBeneficiary(vault)                        # the IGP's claim() is permissionless
```

### 4.3 Solana (Phase 4)

```bash
cd svm && cargo build-sbf
solana program deploy target/deploy/rrv.so
solana program deploy target/deploy/igp_oracle_governor.so

# 1. Init the rrv → the config PDA ("rrv-config") is the POOL:
#    register it as the IGP beneficiary (via governor: SetIgpBeneficiary)
# 2. Init the governor (multisig, operators, quorum, epoch, delta, igp_program, igp)
# 3. multisig: SetDomainConfig per domain (bounds + token_decimals — scale 1e19!)
# 4. ⚠️ TEST TransferIgpOwnership ON DEVNET before step 5 (spec §08)
# 5. real IGP: TransferIgpOwnership(governor's config_pda)
# 6. ⚠️ upgrade authority of BOTH programs → multisig:
solana program set-upgrade-authority <PROGRAM_ID> --new-upgrade-authority <MULTISIG>
# 7. keep lamports on the governor's config PDA (the IGP realloc charges the owner)
```

### 4.4 oracle-agent (all chains)

```bash
cd oracle-agent && cp config.example.json config.json
# fill in: governors, RPCs, domains (TC = 132556), gas sources
TC_MNEMONIC=... EVM_PRIVATE_KEY=... SOLANA_KEYPAIR_PATH=... npm run once   # cron 6h/6h
```

Each operator runs THEIR agent with THEIR key — no coordination; the governor
converges via the median.

---

## 5. Operation and monitoring

| What | How | Alarm |
|---|---|---|
| TC Mailbox layout | query `{"layout_check":{"message_id":"<delivered id>"}}` on the vault | `ok:false` with "VALUE LAYOUT MISMATCH" → migrate changed the layout; PAUSE claims |
| Pool solvency | query `{"solvency":{}}` (TC) · `claimsPayable()` (EVM) | capacity < delivery backlog |
| Stuck Solana epoch | 2+ divergent hashes in the `EpochState` | manual audit of the reports vs public chain |
| Price not applied | `Applied{domain,epoch}` empty after the epoch | quorum did not converge or `DeltaExceeded` → consider `ForceSet` |

## 6. Parameters to define IN THE PROPOSAL (spec §14)

Fee per delivery on each network · claim window · oracle bounds per
domain/network · operator addresses + quorum · multisig composition/threshold
(with signers that are NOT Hyperlane validators) · ISM threshold
(3-of-4) · timelock for ISM swap.
