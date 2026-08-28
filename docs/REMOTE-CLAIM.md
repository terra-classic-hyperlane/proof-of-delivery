# ClaimRemote (Vault v2) — how the 4 chains tie together

**For those who operate a relayer on TC + BSC + ETH + Solana.** v2 makes the TC
vault pay, in LUNC, for the deliveries that YOUR addresses made on the remote
chains — using the fact that **the message_id is the same on both ends** of each
message.

## 1. The binding (the identity map)

A single operator owns different addresses on each chain. The TC vault keeps this
map (`SetRemoteBinding`, editable only by the owner — later, governance):

```
                         VAULT v2 on Terra Classic
                terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
                                     │
     operator terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp (you)
                                     │ bindings per domain:
        ┌────────────────────────────┼─────────────────────────────┐
        ▼                            ▼                             ▼
  domain 56 (BSC)             domain 1 (ETH)            domain 1399811149 (Solana)
  0x8f085bad…5291             0xef818120…00ae           PbEo7Fn2…cwwrkS
  (BSC relayer)               (ETH relayer)             (Solana relayer)
```

## 2. The full cycle of a TC → remote message

```
1. user sends IGORFAKE from TC to BSC
   └─ pays the fee in LUNC → IGP → (automatic Sweep) → vault POOL
2. YOUR BSC relayer (0x8f08…) executes process() on the BSC Mailbox
   └─ the delivery's message_id = the SAME as the dispatch on TC
3. the claim-agent (off-chain, on the VPS) VERIFIES the delivery on BSC
   └─ ProcessId event on the Mailbox + mailbox.processor(id) == 0x8f08…
4. the claim-agent ATTESTS on the TC vault:
   AttestRemoteDelivery { domain: 56, message_ids: [id] }
5. the vault checks: registered attestor ✓ · binding (you, 56) = 0x8f08… ✓
   · id never paid ✓ · quorum reached ✓ → PAYS the domain reward
   in LUNC to terra1run9wz…  ←  the origin fee returned to the operator
```

The same applies to ETH and Solana — only the domain and the bound address
change. (And delivery ON TC ITSELF still uses the classic `Claim`, by direct
proof.)

## 3. The final economics: ONE payment per delivery, on the ORIGIN chain

**Decision of 08/19 (owner):** the DESTINATION reward was the compensation of the
old architecture (when the origin fee did not reach the executor). With v2,
paying on both ends was DOUBLE payment — so the destination rewards were reduced
to 1 symbolic unit (1 uluna / 1 wei / 1 lamport; the contracts do not accept
zero) and the claim-agent no longer spends gas on them (`localClaim: false`). The
REAL payment is single:

| Message origin | Executor receives | Mechanism |
|---|---|---|
| TC | 33 LUNC | per id (`AttestRemoteDelivery`) |
| BSC | ≈ real fee in BNB | per id (`attestRemoteDelivery`) |
| ETH | ≈ real fee in ETH | per id (`attestRemoteDelivery`) |
| Solana | 499,000 lamports | per epoch (`EpochReport.remote`) |

Note: with a symbolic destination reward, THIRD-PARTY relayers (without a
binding) no longer have an incentive — a deliberate decision of the current phase
(1 operator). To reopen competition, governance restores the destination rewards
(`update_config`/`setParams`/`SetRewardLamports`).

And, symmetrically, each ORIGIN chain pays the fee to the executor — **per id**
(individual record per message) on the chains where storing is cheap, and
**per epoch** (a 6 h aggregate in the report) on Solana, where the rent per
account would cost more than the fee (detail: `EXPANSION-MANUAL.md` §2):

| Origin | Mechanism | Value per delivery |
|---|---|---|
| TC | per id (`AttestRemoteDelivery`) | 33 LUNC |
| BSC | per id (`attestRemoteDelivery`) | 0.0000023 BNB (real fee) |
| ETH | per id (`attestRemoteDelivery`) | 0.0000093 ETH (real fee) |
| Solana | per epoch (`EpochReport.remote`) | 0.000499 SOL (real fee) |

## 4. The trust model (honest)

TC **cannot see** the other chains. v2 does not change this — it replicates the
model already approved for the Solana vault (which also does not record the
executor): **quorum of registered attestors**. The guardrails that limit abuse:

1. **Prior binding**: it only pays a remote address that the owner/governance
   bound;
2. **1 payment per message_id** (`REMOTE_CLAIMED`, effects-first) — an id never
   pays twice;
3. **Fixed reward per domain** — a fake id costs at most 33 LUNC, never the pool;
4. **Quorum of AGREEING attestations** (same executor). Today = 1
   (self-attestation — acceptable because owner = single operator = you, in
   testing). **With 2+ independent operators, raise to ≥ 2**
   (`SetRemoteOperators`);
5. **Public audit**: `RemoteAttestations{message_id}` shows who attested what;
   anyone can check the message_id on both chains (it is the same hash);
6. `SetPause` freezes everything in an emergency.

## 5. Operation (commands)

```bash
V=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
TX="--from operador --gas auto --gas-adjustment 1.4 --gas-prices 28.325uluna --chain-id columbus-5 --node https://rpc.terra-classic.hexxagon.io -y"

# owner: attestors + quorum · bindings · reward per domain
terrad tx wasm execute $V '{"set_remote_operators":{"attestors":["terra1..."],"quorum":1}}' $TX
terrad tx wasm execute $V '{"set_remote_binding":{"operator":"terra1...","domain":56,"remote_address":"0x..."}}' $TX
terrad tx wasm execute $V '{"set_remote_reward":{"domain":56,"reward":"33000000"}}' $TX

# attestor: attests deliveries (the claim-agent does this on its own)
terrad tx wasm execute $V '{"attest_remote_delivery":{"domain":56,"message_ids":["<id_hex64>"]}}' $TX

# audit queries
terrad q wasm contract-state smart $V '{"remote_config":{}}' --node <NODE>
terrad q wasm contract-state smart $V '{"remote_claimed":{"message_id":"<id>"}}' --node <NODE>
terrad q wasm contract-state smart $V '{"remote_attestations":{"message_id":"<id>"}}' --node <NODE>
```

## 6. Automation (claim-agent)

The claim-agent already verifies the deliveries on the 4 chains; with v2 it
gained the final step: every REMOTE delivery confirmed as yours enters a queue
(`state.json → remoteAttest`) and is attested on the TC vault in the same hourly
round — log: `✓ attested dom <n> → <tx>`. No manual action.

## 7. v2 deployment

Reproducible build `cosmwasm/optimizer:0.17.0` → `relayer_reward_vault.wasm`
sha256 `e24a5e66ab4a503c6acf369710b717310362d2ae5fa7b9800542c8272b2fc801`.
Migration **at the same address** EXECUTED on 08/19/2026 (code_id **11589**,
store `A9866AEE…`, migrate `C4075BA8…`) via `deploy/tc-migrate-vault-v2.sh`
(LOCAL — project rule: no wasm/deploy on the VPS). First payments:
99 LUNC for the day's 3 deliveries (txs in `AUDIT-TC.md`).
30 green tests (5 new in v2: quorum 1 pays, anti-double, quorum 2 waits for
agreement, rejections, totals). Execution record: `AUDIT-TC.md`.

## 8. v2 also on BSC and ETH (mirror of the model)

The EVM vaults gained the SAME module (`attestRemoteDelivery` etc., 38 foundry
tests). Since the EVM contracts are not migratable and the pools were empty,
v2 is a **new deploy** + `igp.setBeneficiary(v2)` — LOCAL script
`deploy/evm-vault-v2.sh bsc|ethereum`, which also configures: attestor = owner,
quorum 1, binding `(owner, 132556) → terra1run9wz…` and **reward = the real IGP
quote** (`quoteGasPayment(132556, destinationGas)`) — exactly the fee the user
pays at origin.

Mirrored flow: user dispatches FROM BSC → pays the fee in BNB → your relayer
delivers ON TC → claim-agent detects the delivery on TC (the event carries the
ORIGIN), enqueues and attests on the BSC vault → **the fee in BNB returns to the
operator**. The same for ETH. Thus, ALL 4 chains pay the origin fee to the
executor.

## 9. v2 on Solana (via epoch report)

On Solana, a PDA per message would cost more rent (~0.0015 SOL) than the fee
itself (0.000499 SOL). That is why the `EpochReport` gained the field `remote:
[(domain, operator, count)]` — the remote credits go through the SAME
hash/quorum of the report and are paid out through the normal `Withdraw`, with
zero extra cost. Config via administrative proposal: reward `499,000 lamports`
(measured real fee) and binding `(132556, PbEo7Fn2…) → terra1run9wz…`. The
claim-agent aggregates the deliveries of Solana→TC msgs per epoch and includes
them in the report automatically.

## 10. Sustainability (owner/governance attention)

v2 pays from the SAME pool as the local `Claim`. With a 33 LUNC reward ≈ the
average origin fee, the pool stays ~neutral (fee in, reward out). If governance
raises the remote reward too much, the pool drains — monitor `total_remote_paid`
vs the Sweep collection (`RemoteConfig{}` + `Solvency{}`).
