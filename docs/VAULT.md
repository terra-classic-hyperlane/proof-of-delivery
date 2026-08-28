# Vault — how to query, configure, and operate

Operational guide for the RelayerRewardVault across the 4 networks: what the
**owner** can change, how the **relayer** withdraws, and the monitoring queries.

## What is configurable (and by whom)

| Parameter | What it controls | Who changes it |
|---|---|---|
| `reward_per_delivery` | fixed fee paid per proven delivery | owner (TC/EVM) · quorum (SOL) |
| `claim_window_blocks` | deadline to claim after delivery | same |
| `paused` | blocks claims (emergency) | same |
| `owner` | who administers (→ governance/multisig at handoff) | current owner |
| `mailbox` / `igp` (TC only) | sources of the proof and of the Sweep | owner |
| surplus withdrawal | withdraw from the pool to a destination | owner (TC/EVM) · quorum with destination in the hash (SOL) |

What **nobody** configures: who can claim the reward — the `Claim` pays
exclusively whoever the chain's registry points to as the executor of the delivery.

---

## Terra Classic

**Query:**
```bash
terrad q wasm contract-state smart <VAULT> '{"config":{}}'    --node $NODE   # everything, incl. total paid
terrad q wasm contract-state smart <VAULT> '{"solvency":{}}'  --node $NODE   # pool and how many deliveries it can fund
terrad q wasm contract-state smart <VAULT> '{"claimed":{"message_id":"<hex64>"}}' --node $NODE
terrad q wasm contract-state smart <VAULT> '{"layout_check":{"message_id":"<delivered id>"}}' --node $NODE  # migrate alarm
```

**Configure (owner signs — today the deployer; later, a governance proposal):**
```bash
# fee and/or window (fields are optional — send only what changes):
terrad tx wasm execute <VAULT> '{"update_config":{"reward_per_delivery":"75000000","claim_window_blocks":300000}}' $TXFLAGS
# pause / unpause:
terrad tx wasm execute <VAULT> '{"set_pause":{"paused":true}}' $TXFLAGS
# withdraw surplus:
terrad tx wasm execute <VAULT> '{"withdraw_surplus":{"to":"terra1...","amount":"1000000000"}}' $TXFLAGS
# owner handoff (governance):
terrad tx wasm execute <VAULT> '{"update_config":{"owner":"<GOV_MODULE>"}}' $TXFLAGS
```

**Relayer usage (permissionless):**
```bash
# pulls the IGP collection into the pool and claims in the SAME tx (atomic batch):
terrad tx wasm execute <VAULT> '{"sweep":{}}' $TXFLAGS
terrad tx wasm execute <VAULT> '{"claim":{"message_ids":["<hex64>","<hex64>"]}}' $TXFLAGS
```

**Fund the pool:** any `bank send` of uluna to the vault address.

---

## BSC / Ethereum

**Query:**
```bash
cast call $VAULT "rewardPerDelivery()(uint256)" --rpc-url $RPC
cast call $VAULT "claimWindowBlocks()(uint256)" --rpc-url $RPC
cast call $VAULT "paused()(bool)"               --rpc-url $RPC
cast call $VAULT "claimsPayable()(uint256)"     --rpc-url $RPC   # solvency
cast call $VAULT "claimedBy(bytes32)(address)"  0x<id> --rpc-url $RPC
cast call $VAULT "totalPaid()(uint256)"         --rpc-url $RPC
```

**Configure (owner signs):**
```bash
cast send $VAULT "setParams(uint256,uint256)" <REWARD_WEI> <WINDOW> --private-key $PK --rpc-url $RPC
cast send $VAULT "setPause(bool)" true --private-key $PK --rpc-url $RPC
cast send $VAULT "withdrawSurplus(address,uint256)" 0xDEST <WEI> --private-key $PK --rpc-url $RPC
# handoff (2 steps — the multisig must ACCEPT):
cast send $VAULT "transferOwnership(address)" 0xMULTISIG --private-key $PK --rpc-url $RPC
# ... and the multisig executes: acceptOwnership()
```

**Relayer usage:** `igp.claim()` (permissionless, pushes the collection to the
vault) and `vault.claim(bytes32[] ids)` — can go in the same tx via its own multicall.

**Fund the pool:** transfer BNB/ETH directly to the vault (`receive()` accepts it).

---

## Solana (rrv)

Here there is NO single owner: changes are **proposed with an operator quorum**
(`AdminEnvelope { nonce, action }` — see `docs/OPERADORES.md` §Solana):

| Action | Envelope |
|---|---|
| fee | `SetRewardLamports(u64)` |
| pause | `SetPaused(bool)` |
| epoch duration | `SetEpochDuration(u64)` |
| operators/quorum | `AddOperator/RemoveOperator/SetQuorum` |
| surplus | `WithdrawSurplus { to, amount }` — the **destination is part of the hash**: THAT destination is approved |

**Query:** read the config PDA `["rrv","-","config"]` (the init prints the
address) — the PDA's lamport balance above the rent-exempt IS the pool. Credits
per operator: PDA `["rrv","-","credit","-",<operator>]`.

**Operator usage:** `SubmitEpochReport` (epoch report, a quorum of identical
hashes credits) and `Withdraw { amount }` (direct debit from the pool,
limited to one's own credit and to the rent-exempt).

**Fund the pool:** transfer SOL to the config PDA (and register the PDA as the
IGP beneficiary — done in the deploy's `finalize`).

---

## Minimal monitoring (alarms)

| Signal | Where | Action |
|---|---|---|
| `layout_check.ok = false` (TC) | query on the vault | migrate on the Mailbox — **pause** and investigate |
| `claims_payable` dropping < backlog | Solvency / claimsPayable | Sweep/claim from the IGP is not running, or fee > collection |
| claims reverting `NotProcessor` | relayer logs | relayer using the wrong wallet for the claim |
| frequent `ClaimWindowExpired` | logs | relayer claiming late — automate the claim after delivery |
