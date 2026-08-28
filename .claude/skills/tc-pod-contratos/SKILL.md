---
name: tc-pod-contratos
description: >-
  DEVELOPMENT runbook for the tc-proof-of-delivery contracts (paying Hyperlane
  relayers across 4 networks): vault + oracle-governor CosmWasm, Vault/Governor
  Solidity, Solana programs (rrv + igp-oracle-governor) and their tests. Use when
  changing, reviewing, testing or extending any contract/program in this repo.
---

# tc-proof-of-delivery — contracts (development runbook)

> Design source of truth: `SPEC.html` (v3) · diagrams: `docs/ARCHITECTURE.md`. Mainnet evidence and test
> scoreboard: `README.md`. Build/deploy: `docs/INSTALL-AND-RUN.md`.

## Non-negotiable principle
The operator is paid for what it **DELIVERED**, proven by the chain's own record:
- **TC**: raw query on the Mailbox storage — key `[0x00,0x0A]+"deliveries"+message_id`,
  JSON value `{"sender","block_number"}` (CONFIRMED in production, code_id 11371);
- **EVM**: `mailbox.processor(id)` / `processedAt(id)` (Mailbox v3);
- **Solana**: the chain does NOT record the executor → operator quorum per epoch.
Not a single line of the Hyperlane core is modified — only configuration (beneficiary/owner).

## Code map
| Layer | Where | Test |
|---|---|---|
| Vault TC | `contracts/relayer-reward-vault/` | `cargo test` (24) |
| OracleGovernor TC | `contracts/oracle-governor/` | `cargo test` (15) |
| EVM | `evm/src/*.sol` + `evm/test/` | `cd evm && forge test` (32) |
| Solana | `svm/programs/{relayer-reward-vault,igp-oracle-governor}` | `cd svm && cargo test` (15) |
| mock IGP (TEST ONLY) | `svm/programs/mock-igp` | mirrors borsh indices 5/7/9 + the real IGP's accounts |

## Invariants the tests protect (do not break)
1. **Atomic claim**: an invalid id reverts the batch; nothing is consumed.
2. **Effects-first**: the payment record is written BEFORE the transfer.
3. **STRICT parse on TC** (`deny_unknown_fields`): a Mailbox migrate → error
   `MailboxLayoutMismatch`, never a wrong payment. Monitor via the `LayoutCheck` query.
4. **Median = lower of the central ones** on an even tie (charges the user less).
5. **Bounds belong to governance/multisig, never to the operators** (conflict of interest).
6. **Max delta (bps)** vs last applied; exceeded → only an emergency resolves it.
7. Solana §09: the slot window **locks on the 1st submission**; credit list
   **strictly ordered**; `WithdrawSurplus` destination **inside the hash** of the envelope.
8. `Sweep` (TC) permissionless; EVM does not need it (the IGP claim is permissionless + `receive()`).
9. Scales: exchange_rate 1e10 (CW/EVM) vs **1e19 (Solana)** — never copy bounds between VMs.

## External interfaces (do NOT invent — they were checked against the real repos)
- IGP CW (`~/tc-cw-hyperlane`): `{"claim":{}}` beneficiary only; oracle ownership in
  2 steps `InitOwnershipTransfer`→`ClaimOwnership`; `SetRemoteGasData{config}`.
- EVM (`~/hyperlane-monorepo/solidity`): `StorageGasOracle` is OZ Ownable, single step.
- Solana (`~/hyperlane-monorepo/rust/sealevel`): IGP borsh instruction —
  Transfer=5, SetBeneficiary=7, SetGasOracleConfigs=9; accounts 9=[system, igp w,
  owner signer], 5/7=[igp w, owner signer]; `RemoteGasData` has `token_decimals`.

## Toolchain / pitfalls
- rustc 1.84: the `Cargo.lock` files have ~20 anti-edition2024 pins — do **not** run
  a broad `cargo update`; update with a targeted `--precise`.
- EVM compiles with `via_ir = true` (stack too deep on submitPrice without it).
- Solana: `unexpected_cfgs` lints allowed in the workspace (false positive from
  entrypoint! 1.18); `cargo build-sbf` produces the `.so` files.
- Before any PR: `cargo test` + `clippy -D warnings` (2 workspaces) +
  `forge test` — 91 tests in total, all green.
