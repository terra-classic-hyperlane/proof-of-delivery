# Expansion Manual — new chain, new operators, new associations

How to grow the system without breaking anything, in the **trustless receipt model**
(`RECIBO-TRUSTLESS.md`). Three operations: **register identity (mapping)** ·
**add operator** · **add chain**. Current addresses:
`REGISTRO-AUDITORIA.md`.

---

## 1. What the "association (mapping)" is — the identity registry

An operator is ONE identity with a DIFFERENT address on each chain. The registry
stores this by INDEX — the same row applies across all chains:

```
operator 0 = { TC: terra1run9wz…,  BSC: 0x8f08…,  ETH: 0xEF81…,  SOL: PbEo… }
operator 1 = { TC: terra1abc…,     BSC: 0x1234…,  ETH: 0x5678…,  SOL: 9xYz… }
```

Each vault stores this registry (`operator_address[(index, domain)]`) and a
**reverse-lookup** for the LOCAL domain (`address → index`). The receipt carries
only **(message_id, index)**; each chain pays the address of the index in ITS own
registry. This is the trust trade-off of the trustless model: the validators prove
THE DELIVERY (in the receipt), the registry (only the owner writes) states THE
IDENTITY/where to pay.

### Commands (owner) — see §3.2 for the per-chain step by step

```bash
# TC
terrad tx wasm execute $VAULT '{"set_operator_address":{"index":<N>,"domain":<dom>,"address":"<address>"}}' $TX
# BSC/ETH
cast send $VAULT "setOperatorAddress(uint32,uint32,string)" <N> <dom> "<address>"
# remove: address null (TC) / "" (EVM) in the domain
```

---

## 2. By id vs by epoch — the two payment modes for the origin fee

Payment is always **on the ORIGIN chain** (the one that charged the fee), to the
**operator that DELIVERED** (proven by `processor(id)` at the destination + receipt
signed by the validators). Only how the registry is stored changes:

| Mode | Where | How it works | Why |
|---|---|---|---|
| **By id** | TC, BSC, ETH | Each `message_id` is paid individually upon receiving the receipt (`handle`). `remote_claimed[id]` guarantees 1 payment per message, auditable id by id. | Storing 1 record per id costs cents — the maximum granularity is worth it. |
| **By epoch** | Solana | Deliveries AGGREGATED by a 6 h window in `EpochReport.remote` (count × reward); withdrawal via `Withdraw`. | On Solana 1 account per id would cost ~0.0015 SOL of rent — MORE than the fee. Aggregating zeroes the cost. |

Current table (remote reward ≈ real origin fee):

| Origin | Mechanism | Value per delivery |
|---|---|---|
| TC | by id (receipt → `handle`) | 33 LUNC |
| BSC | by id (receipt → `handle`) | ≈2.26e12 wei (real fee, recalibratable) |
| ETH | by id (receipt → `handle`) | ≈9.29e12 wei (real fee) |
| Solana | by epoch (`EpochReport.remote`) | 499,000 lamports (real fee) |

> **Current model = TRUSTLESS RECEIPT** (`RECIBO-TRUSTLESS.md`): the destination vault
> proves the delivery and dispatches a receipt signed by the validators; the origin
> vault pays on receipt. **No attesters, no quorum, no agent with decision-making
> power.** The previous attestation model (with quorum) is described in
> `CLAIM-REMOTO.md`/`SEGURANCA-CLAIMREMOTO.md` for historical reference.

---

## 3. Adding an OPERATOR (receipt model)

In the receipt model **there is no quorum or attestation** — each operator is
INDEPENDENT: whoever delivers, receives (the `processor(id)` proves who it was, the
mapping registry says where to pay). Adding an operator = just **register their
addresses** (the owner writes; one day the frontend will do this on a screen).

### 3.1 The new operator prepares
- 1 address per chain (TC/BSC/ETH/Solana) with a minimum balance for gas;
- runs their own Hyperlane relayer (to deliver and earn the origin fees).

### 3.2 The owner registers the mapping (new index, e.g. 1) — in EACH vault

Rule: in each vault, the address of the **LOCAL domain** feeds the reverse-lookup
(this is how the destination discovers "whoever delivered here is operator N"); the
other domains are the registry of where to pay on the origin.

```bash
# --- TC vault (local dom 132556) ---
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":132556,"address":"terra1novo..."}}' $TX  # local
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":56,"address":"0xNOVO_BSC..."}}' $TX        # registry
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":1,"address":"0xNOVO_ETH..."}}' $TX

# --- BSC vault (local dom 56) ---
cast send $VAULT_BSC "setOperatorAddress(uint32,uint32,string)" 1 56     "0xNOVO_BSC..."     # local → reverse-lookup
cast send $VAULT_BSC "setOperatorAddress(uint32,uint32,string)" 1 132556 "terra1novo..."     # registry

# --- ETH vault (local dom 1) --- same as BSC, swapping 56→1
# --- Solana (pod) --- administrative proposal (multisig):
#   AdminAction::SetRemoteBinding{ domain, operator: <pubkey>, remote_address } — model in deploy/rrv-remote-config.mjs
```

### 3.3 Verification
```bash
# reverse-lookup: does the local executor of operator 1 resolve to index 1?
cast call $VAULT_BSC "operatorOfLocal(address)(bool,uint32)" 0xNOVO_BSC...
terrad q wasm contract-state smart $VAULT_TC '{"operator_of_local":{"address":"terra1novo..."}}' --node $NODE
```
A test delivery from operator 1 → the receipt pays their address
(`remote_claimed[id].executor` = address of N on the origin). No other step.

### 3.4 Removing an operator
`set_operator_address` with an empty/`null` address in the local domain removes the
reverse-lookup (they are no longer recognized as an executor); remove them in the
other domains to clean up the registry.

---

## 4. Adding a CHAIN (a new network joins the bridge)

Prerequisite: the warp/mailbox/IGP/ISM of the new network already deployed (outside
the scope of this system). Then, 6 steps:

1. **This system's vault on the new network** (same contract, both roles):
   - EVM: `deploy/evm-vault-receipt.sh <chain>` (deploy + beneficiary + config);
   - CosmWasm: pattern of `deploy/tc-migrate-vault-receipt.sh`;
   - SVM: same `pod` program (no new deploy — just config).
   In all: `IGP.beneficiary → vault`.
2. **Cross router** — each pair of vaults registers the other as a trusted
   router: `set_remote_router{domain, <the other's vault>}` (the address is the
   canonical 32B / hex32 left-pad). This is what authorizes the `handle` and defines
   the target of `send_receipt`. **Without a mutual router, the receipt is neither
   accepted nor dispatched.**
3. **Rewards** — in each origin vault, `set_remote_reward{<dest_dom>,
   <real fee>}` for the destinations it starts to serve (production is the truth).
4. **operator mapping** — register each operator's address on the new
   network in ALL vaults (§3.2), in both directions.
5. **Hyperlane route** — the vault needs to be a valid recipient and the entry ISM
   must accept the receipt's route. Corridors that already have a bidirectional warp
   (the ISM already validates both directions) use the default ISM — no extra config.
   A new route requires registering its ISM/validators (infra config, without
   touching a native Hyperlane contract).
6. **oracle-agent** — new block in `config.json` (price). The ATTESTATION
   claim-agent is no longer necessary in the receipt model; the operator (or the
   frontend) calls `send_receipt` when the `quote` is worth the gas.
7. **Audit** — `docs/AUDITORIA-<CHAIN>.md` + update `REGISTRO-AUDITORIA.md`.

---

## 5. Golden rules (learned in production)

1. **Production is the truth** — bounds, price and reward are derived from the
   current on-chain value, never from documentation (it ages).
2. **The receipt model has no quorum** — each operator is independent; the proof is
   the `processor(id)` + the receipt signed by the validators. An operator only
   receives for deliveries THEY made, at the address the OWNER registered.
3. **Mutual router is mandatory** — the two vaults of the corridor must register
   each other (`set_remote_router`); it is the allowlist that makes the `handle` safe.
4. **Deploys are LOCAL** — the VPS only runs the relayer/validator binaries and the
   oracle-agent; wasm/deploy scripts never go there.
5. **Remote reward ≈ real fee** — keeps the pool neutral. Monitor
   `total_remote_paid` vs collection (Sweep/IGP) per chain.
6. **A synchronized relayer is money** — relaying is permissionless; delayed
   indexing = race lost to competitors (it happened: `EaxLm3Hw…`).
7. **The operator decides when to withdraw** — no on-chain threshold; check `quote`
   and group deliveries (batching) until the receipt is worth the gas.
