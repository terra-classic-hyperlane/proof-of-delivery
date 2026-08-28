# Commission Automation — claim-agent + epoch-reporter

> Two off-chain agents that emit/report commissions automatically across all
> chains. **No contract is changed; no relayer is customized** — the agents only
> observe the chain and fire the transactions any operator would already do by hand.

Summary:
| Corridor | How the commission is claimed | Tool |
|---|---|---|
| TC→BSC, BSC→TC, Solana→TC | **receipt model** (emits a receipt on the destination) | `claim-agent-receipt.mjs` |
| **TC→Solana** | **operator quorum** (epoch report) | `solana-epoch-reporter.mjs` |

Why two: in the receipt model the destination **proves delivery on-chain** and the ISM
validates. This works where the destination records the executor (TC/BSC/ETH). Solana **does
not record the executor**, so the TC→Solana direction uses **quorum**: operators observe
off-chain who delivered and submit the same report; the honest majority credits.

---

## 1. `claim-agent-receipt.mjs` — receipt model (TC↔BSC + Solana→TC)

For each DESTINATION chain, finds the deliveries made **by the operator** that have not
yet been paid, batches them by origin, and emits the receipt:
- **TC** (`send_receipt`) → pays **BSC→TC** (BNB) and **Solana→TC** (SOL)
- **BSC** (`sendReceipt`) → pays **TC→BSC** (LUNC on TC)
- **ETH** — once the ETH vault exists (auto-skip)

The commission always lands on the **origin** chain; the agent only fires. The native relayer
delivers the receipt back and the origin pays on its own.

**Run:**
```bash
# see what it would do (no keys, read only):
DRY=1 node deploy/claim-agent-receipt.mjs
# emit 1 round (needs the keys):
BSC_PRIVATE_KEY=0x… TC_KEYRING_PASS='password' node deploy/claim-agent-receipt.mjs
# service (every 5 min):
BSC_PRIVATE_KEY=0x… TC_KEYRING_PASS='password' node deploy/claim-agent-receipt.mjs --loop 300
```

Details:
- **Discovery without `getLogs`** (the BSC public RPC does not support it): scans the TC
  dispatches (`tx_search`) and confirms state with `eth_call`/query.
- **Dedup by origin** (avoids re-emitting and wasting gas): TC→BSC checks `remote_claimed` on
  TC; BSC→TC checks `remoteClaimed` on BSC; Solana→TC uses local state
  (`deploy/.claim-agent-seen.json`) + the on-chain idempotence of `send_receipt`.
- **Excludes receipts** (recipient == vault) so it does not "receive a commission on a receipt".
- **Batching**: joins N deliveries from the same origin into a single receipt (1 gas).

> Savings: each receipt costs gas; batch several deliveries. The Terra tax (~1.5%)
> is charged 1× per outgoing transfer, not per id — one more reason to batch.

---

## 2. `solana-epoch-reporter.mjs` — quorum (TC→Solana)

The NATIVE relayer delivers the TC→Solana messages (nothing changes). The reporter **observes
off-chain who delivered** (fee payer of the delivery tx, read from `ProcessedMessage`), builds
the `EpochReport` and submits it to the `pod`. When a **quorum** of operators submits the SAME
report (identical hash), the contract credits each operator; each one **withdraws** from the pool.

Deterministic for the quorum: each delivery is assigned to an epoch by the `blockTime` of
its slot (all read from the chain), so every operator arrives at the SAME report.

**Trust:** honest majority of the quorum — **the same one you already place in the
validators** (and where operator = validator, it is the same group). It is not the ISM's
cryptographic proof (impossible here, Solana does not record the executor), but it is
decentralized and has no single agent.

**Run:**
```bash
# see the report for the last closed epoch (read only):
node deploy/solana-epoch-reporter.mjs
# for a specific epoch:
node deploy/solana-epoch-reporter.mjs --epoch 82736
# submit (signs as an rrv operator; each quorum operator runs this):
node deploy/solana-epoch-reporter.mjs --submit
# withdraw afterward: the operator withdraws from its credit PDA (pod Withdraw)
```

Details:
- **Credits only registered operators** (`config.operators`) by default — the pool does not
  pay unknown relayers. `INCLUDE_ALL=1` credits anyone who delivered (permissionless
  mode).
- **Activation:** quorum ≥ majority (with 2 operators, `quorum=2` to be genuinely
  trustless; with `quorum=1` it is a single operator) and `reward_lamports` > 0 (**today it is 1,
  a placeholder — adjust**). Config via administrative action of the `pod` (governance).

> **PROVEN IN PRODUCTION:** the operator `PbEo7Fn2…` was **credited 6,000,000 lamports and
> withdrew the 6,000,000** via this mechanism — the TC→Solana cycle (native delivery →
> quorum report → credit → withdrawal) works end to end.

---

## Rules both respect
- Hyperlane native relayer **unchanged** (the agents do not deliver messages).
- **No native Hyperlane contract** touched.
- **No keeper** (customized relayer). The reporter is an observer, not a relayer.
- Scales to **N operators**: each one runs the agent(s); at quorum, the honest
  majority credits.

Addresses and conversions: `OPERATORS-VALIDATORS-GUIDE.md`. Receipt model:
`TRUSTLESS-RECEIPT.md`. Payment audit: `AUDIT-COMMISSIONS.md`.
