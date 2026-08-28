# Trustless Receipt — step-by-step commands

Trustless model: the DESTINATION vault proves delivery on-chain and dispatches a
receipt signed by the bridge validators; the ORIGIN vault pays on receipt.
No attester, no agent with decision power — immune to a malicious relayer.

> **Status: PROVEN IN PRODUCTION (08/19/2026), TC↔BSC corridor in both directions.**
> Since 08/20/2026 (code_id 11596): `SendReceipt{gas_limit}` — the receipt leaving
> TC pays only the REAL GAS via IGP metadata, not the $0.08 user fee
> (which would eat up the commission). See `FEES-AND-REWARDS.md` §receipts.
> Receipt vaults: TC `terra1gqkrh2…` (code_id 11592) · BSC
> `0x34E06a7793877EC5251b1dC230aD7cD577d231f4` (ism = warp ISM; in the proof it was
> `0xa82087B8…`, since 08/20/2026 it is the mutable `0xF6b0cDD3…` — `ISM-VALIDATORS.md`).
> Proofs: BSC→TC paid 33 LUNC (tx `F4700EF4…`, msg `974a7e47…`); TC→BSC paid
> 2,259,538,750,000 wei BNB (msg `5920d3fb…`, receipt `b6d00d74…`). Hyperlane
> integration details in §F.
>
> **Solana:** the **Solana→TC** corridor is PROVEN IN PRODUCTION (2026-08-19), keeperless
> (native relayer) — design, formats, step-by-step and proofs in **§G**.
> TC→Solana is out of scope (it would require a custom relayer).

## Implementation plan (phases)

1. **Global to/from registry** (Phase 1, IN PROGRESS) — each vault stores
   `operator N → {address per domain}` (only the owner writes) + reverse-lookup
   `address → N`. Consolidates the per-corridor bindings into a single identity registry.
2. **`send_receipt`** (DESTINATION role) — proves delivery (`processor(id)`), reads the
   origin domain from the MESSAGE (committed by the `message_id`), resolves the
   executor's operator N and dispatches the receipt through the Mailbox. The operator pays the gas.
3. **`handle`** (ORIGIN role) — accepts ONLY from the Mailbox + registered router; for
   each `(id, N)` pays the address of N in its OWN local registry (never an
   address coming in the receipt); once per id.
4. **Hyperlane routing** of the TC↔BSC corridor (infra config, without touching the
   native contract).
5. **TC→BSC end-to-end test** → then **BSC→TC**.
6. **Replicate** to ETH and Solana. ETH: same contract (deferred due to gas). **Solana:
   only the Solana→TC direction** is possible without a keeper — see §G (the TC→Solana
   direction requires a keeper and was discarded, since the chain does not record the executor).

### Why "receipt by operator INDEX" (not by address)

The `message_id` identifies the MESSAGE, not the executor — whoever delivered is only
recorded at the DESTINATION (`processor(id)`). The receipt carries **(message_id, N)**;
the origin pays the **address of N in its own registry** (set by the owner),
so not even a malformed receipt diverts the payment. The to/from is the identity
backbone, replicated on each chain.

## Concepts

- **The same vault contract** on each chain plays TWO roles depending on direction:
  - ORIGIN (msgs that left it): holds the pool · `handle()` receives the receipt · pays.
  - DESTINATION (msgs delivered to it): `send_receipt` proves delivery and dispatches the receipt.
- **No on-chain threshold** — the operator decides when to send (queries `quote` beforehand).
- **Origin domain = proof** — read from the Hyperlane message itself (not a guess).
- **Binding at the DESTINATION** — `binding[executor_local][origin_domain] → payment address at origin`.
- **The operator pays the gas** of the receipt (recovers it in the reward; hence the batching).

Domains: TC `132556` · BSC `56` · ETH `1` · Solana `1399811149`.

---

## A0. Deploy + config of the TC↔BSC corridor (ready scripts)

Execution order (LOCAL — never on the VPS):
```bash
# 1) deploy the BSC receipt-vault + config the BSC side (uses the TC vault as a constant)
PRIVATE_KEY=0x<bsc_key> bash deploy/evm-vault-receipt.sh bsc
#    → prints BSC_VAULT=0x…  (new address)

# 2) migrate the TC vault (same address, pool preserved) + config the TC side
BSC_VAULT=0x<from_step_1> bash deploy/tc-migrate-vault-receipt.sh
#    → asks for the keyring password (hyperlane-deploy key)

# 3) seed the BSC pool (the TC one already has 5,000 LUNC); any value:
cast send --legacy 0x<BSC_VAULT> --value 5000000000000000 --private-key 0x<bsc> --rpc-url https://bsc-dataseed.bnbchain.org
```
Each script is idempotent (resumes from `.state`) and ends with the on-chain
verification of the routers/rewards/registry. The Hyperlane route of the receipt uses the
default ISM (the TC↔BSC corridor is already validated in both directions by the warp).

Equivalent manual config (reference) below.

## A. One-time setup (owner) — per corridor

For the corridor X→Y (origin X pays; delivery in Y; receipt Y→X):

### A.1 In the ORIGIN vault (X) — who pays
```bash
# trusted router: only accepts a receipt coming from the Y vault (handle allowlist)
# TC:
terrad tx wasm execute $VAULT_X '{"set_remote_router":{"domain":<Y>,"router":"<vault_Y_hex32>"}}' $TX
# EVM:
cast send $VAULT_X "setRemoteRouter(uint32,bytes32)" <Y> <vault_Y_bytes32>
# reward per delivery in domain Y (≈ real origin fee):
terrad tx wasm execute $VAULT_X '{"set_remote_reward":{"domain":<Y>,"reward":"<value>"}}' $TX
cast send $VAULT_X "setRemoteReward(uint32,uint256)" <Y> <value_wei>
```

### A.2 In the DESTINATION vault (Y) — who proves and sends
```bash
# return router: where to dispatch the receipt (the X vault)
cast send $VAULT_Y "setRemoteRouter(uint32,bytes32)" <X> <vault_X_bytes32>
terrad tx wasm execute $VAULT_Y '{"set_remote_router":{"domain":<X>,"router":"<vault_X_hex32>"}}' $TX
# identity BINDING: the Y executor → the address that receives at origin X
cast send $VAULT_Y "setRemoteBinding(address,uint32,string)" <executor_in_Y> <X> "<payment_address_in_X>"
terrad tx wasm execute $VAULT_Y '{"set_remote_binding":{"operator":"<executor_in_Y>","domain":<X>,"remote_address":"<address_in_X>"}}' $TX
```

### A.3 Hyperlane infra (receipt route)
Register the X vault as a **recipient** and ensure that the inbound ISM of X
accepts messages from the Y vault. (Infra config — does not alter Hyperlane contracts.)

---

## B. TC → BSC flow (origin TC pays in LUNC)

You already dispatched (e.g.: `message_id` = `0x<id>`), your relayer delivered on BSC.

### B.1 Query how much it is worth (at the ORIGIN, TC)
```bash
NODE=https://rpc.terra-classic.hexxagon.io
terrad q wasm contract-state smart $VAULT_TC \
  '{"quote_remote":{"domain":56,"message_ids":["<id_hex_without_0x>"]}}' --node $NODE
# → { "amount": "<LUNC to receive>", "payable_count": <n> }
```
Decide: does `amount` comfortably cover the receipt gas? If yes, proceed. If not, accumulate
more deliveries and repeat (batching — 1 receipt covers N ids).

### B.2 Send the receipt (at the DESTINATION, BSC) — operator pays the gas
```bash
# msg.value covers the BSC IGP quote to deliver the receipt on TC
cast send --legacy $VAULT_BSC "sendReceipt(uint32,bytes32[])" 132556 "[0x<id>,0x<id2>]" \
  --value <gas_igp_wei> --private-key $PK --rpc-url https://bsc-dataseed.bnbchain.org
```
The BSC vault: proves `processor(id)` of each id → reads the origin domain of the msg
(132556) → checks the binding → dispatches the receipt to the TC router.

### B.3 The relayer delivers the receipt on TC → automatic payment
No command: the TC vault receives via `handle`, checks the BSC router, and
pays the LUNC to the bound address. To verify:
```bash
terrad q wasm contract-state smart $VAULT_TC \
  '{"remote_claimed":{"message_id":"<id>"}}' --node $NODE
# → { "claimed": true, "executor": "terra1...", "amount": "...", ... }
```

---

## C. BSC → TC flow (origin BSC pays in BNB)

Mirror of B. You dispatched BSC→TC; your relayer delivered on TC.

### C.1 Query (at the ORIGIN, BSC)
```bash
cast call $VAULT_BSC "quoteRemote(uint32,bytes32[])(uint256,uint256)" 132556 "[0x<id>]" \
  --rpc-url https://bsc-dataseed.bnbchain.org
# → (amount_wei, payableCount)
```

### C.2 Send the receipt (at the DESTINATION, TC) — operator pays the gas
```bash
terrad tx wasm execute $VAULT_TC \
  '{"send_receipt":{"domain":56,"message_ids":["<id>"]}}' \
  --amount <gas_igp>uluna $TX
```
The TC vault proves the delivery (raw query DELIVERIES) → reads the origin (56) → binding
→ dispatches the receipt to the BSC router.

### C.3 Automatic payment on BSC
```bash
cast call $VAULT_BSC "remoteClaimed(bytes32)(address,uint32,uint256,uint256)" 0x<id> \
  --rpc-url https://bsc-dataseed.bnbchain.org
# → executor 0x8f08…, domain 132556, value in wei > 0
```

---

## D. Frontend (future)

The front ties together the two steps that live on different chains:
1. lists the operator's still-unpaid deliveries (scans the Mailboxes);
2. calls `quote_remote` at the ORIGIN and shows the accumulated total + the estimated gas;
3. a "Send receipt" button that executes `send_receipt` at the DESTINATION;
4. tracks the `remote_claimed` until payment.

---

## E. Security (why it is trustless)

- The origin vault only accepts `handle` from the **Mailbox** and the **registered router**
  (the destination vault) — a forged receipt is rejected.
- The receipt only exists if the delivery was **proven on-chain** at the destination
  (`processor(id)`), and passed the **validators/ISM validation** on the way back.
- The origin domain is **read from the message** (committed by the `message_id`) —
  there is no way to divert the payment to another chain's pool.
- **1 payment per id** (`remote_claimed`, effects-first) and per-domain cap.
- Compared model (trust × cost): `REMOTE-CLAIM-SECURITY.md` §3.

---

## F. Hyperlane integration — 2 details that only the real chain revealed (08/19)

The vault is a Hyperlane **recipient**. When delivering the receipt, the Mailbox requires two
things from the recipient that the test mocks did not cover:

1. **Respond to the ISM query.** The Mailbox asks `InterchainSecurityModule`
   of the recipient. Without the query, `process()` reverts ("Error fetching ISM address").
   - CW: added the `QueryMsg::IsmSpecifier(...)` variant → returns `{ism:None}`.
   - EVM: `interchainSecurityModule()` already existed.
2. **Point to an ISM that knows the ORIGIN of the receipt.** `ism = None`/`address(0)`
   uses the chain's DEFAULT ISM — which may not know the origin. On TC the default
   already knows BSC (56); on BSC the default does NOT know TC (132556) → error
   `No ISM found for origin: 132556`. Solution: point to the **same ISM of the
   synthetic warp** for that route (since 08/20/2026: BSC `0xF6b0cDD3…`; ETH
   `0x3ba17675…` — `ISM-VALIDATORS.md`), which already validates the messages coming from
   TC. EVM: `setIsm(<warp_ism>)` (owner).

General rule for a new corridor: the vault of EACH chain that RECEIVES receipts points
`ism` to the warp ISM that validates the receipts' origin (= the origin chain's
validators). A corridor with a bidirectional warp → that ISM already exists.

Proven in production 08/19: BSC→TC (receipt → TC, TC default ISM) and TC→BSC
(receipt → BSC, `ism` = warp ISM — `0xa82087B8` at the time; today `0xF6b0cDD3…`).

---

## G. Solana — **Solana→TC** corridor without a keeper

### Why only one direction
Solana's Sealevel Mailbox **does not record who delivered** (the `struct ProcessedMessage`
in `mailbox/src/accounts.rs` only has `discriminator, sequence, message_id, slot` — no
executor). Therefore:

| Direction | Delivery in | Does the chain record the executor? | Keeperless + trustless? |
|---|---|---|---|
| **Solana→TC** | TC (records `DELIVERIES.sender`) | ✅ | ✅ — same as BSC |
| **TC→Solana** | Solana (does not record) | ❌ | ❌ — would require a keeper (custom relayer) → **discarded** |

Since in a Terra Classic project **you do not run a custom relayer**, TC→Solana is
out of scope. Solana→TC uses **only the native relayer** and is trustless.

### Two Solana constraints that changed the design (vs. EVM/CW)
The `pod`'s `handle`, when the native Mailbox calls it, **does not receive a payer** (the
Mailbox only prepends the `process_authority` — see `processor.rs`, the CPI to the recipient
does not pass account 0). Consequences:

1. **Cannot create an account** (no one to pay the rent) → the payment goes to the **PDA
   `operator_sol(index)`** (the index comes in the receipt body, so it is derivable when
   simulating the `HandleAccountMetas`). The operator withdraws later with `WithdrawOperatorSol`.
2. **Cannot dedupe by id** (idem) → idempotency lives in the **`send_receipt`
   of TC** (`SENT_RECEIPT[id]`): the destination does not re-emit a receipt for an already-sent id.
   Combined with the Mailbox guarantee (single delivery per message), there is no double payment.

### The `pod` as a Hyperlane recipient (exact formats from the monorepo source)
- `ism_response()` → `borsh(Some(WARP_ISM))` (33 bytes); the Mailbox reads it as
  `Option::<Pubkey>::try_from_slice` (`processor.rs`).
- `ism_account_metas()` → `SimulationReturnData(vec![])` (our ISM is constant).
- `handle_account_metas()` → `SimulationReturnData([config(w), router(ro),
  reward(ro), operator_sol(index)(w)…])`, all derived only from the message.
- `handle()` → credits `reward` lamports (from the pool = config PDA) into each
  `operator_sol(index)`.

### Addresses (production)
- `pod` (program): `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj`
  · 32B = `0x1a3be2685e7a787a1bedadcc90889b367f8fe72240de5aa43e4c2b88d07776a2`
- vault TC: `terra1gqkrh2…` · 32B =
  `0x402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579`
- Domains: Solana `1399811149` · TC `132556`. Reward: `499000` lamports (measured fee).

### Step-by-step (LOCAL — nothing on the VPS; the keys are yours)
```bash
# 1) deploy the updated pod (recipient interface + WithdrawOperatorSol)
#    build: cargo build-sbf --manifest-path svm/programs/pod/Cargo.toml  → target/deploy/pod.so
solana program deploy svm/target/deploy/pod.so --program-id <pod_keypair>   # or upgrade

# 2) migrate the TC vault (SENT_RECEIPT idempotency) — preserves pool/registry
bash deploy/tc-remigrate.sh                       # wasm sha256 cb753ed7…563f19bd

# 3) config the corridor (both sides)
node deploy/rrv-receipt-config-solana.mjs         # pod: router(TC)+operator_sol(+reward)
bash deploy/tc-receipt-config-solana.sh           # TC: router(Solana)+to/from

# 4) seed the pod pool (config PDA) with some SOL (pays the rewards)
solana transfer Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w 0.05 --allow-unfunded-recipient
```

### Production flow (per operator, with the native relayer)
1. The operator delivers **Solana→TC** messages (native relayer, no change).
2. On **TC**, calls `send_receipt` for its own deliveries (pays the gas; idempotent):
   ```bash
   terrad tx wasm execute $VAULT_TC \
     '{"send_receipt":{"messages":["<msg_hex>","<msg_hex2>"]}}' --amount <gas_igp>uluna $TX
   ```
3. The **native relayer** carries the receipt back to the `pod`, which credits the SOL into the
   PDA `operator_sol(index)`.
4. The operator **withdraws** (rrv variant 6):
   ```
   WithdrawOperatorSol{index, amount}  — accounts: [signer(wallet) w, opsol PDA(index) w]
   ```

### Scaling to N operators
The 3 (or N) run **only the native relayer**. Each calls `send_receipt` for its own
deliveries and withdraws its PDA. They install nothing. The operator index is the one from the
**global to/from** (the same on TC and in Solana's `SetOperatorSol`).

> **Status: PROVEN IN PRODUCTION (2026-08-19).** Solana→TC receipt delivered by the
> NATIVE relayer into the pod, `handle` paid, operator withdrew. No keeper.
>
> Proofs (mainnet):
> - pod upgrade: `24bTjQSAQpARHA3gKiiT8W7qRPLBMBPftabf3ppijXL6DSazNmVsD7Xsoi2GxRdD8hd7q3rpKZZa8TyGD739QF22`
> - TC vault migrate → `code_id 11594` (wasm `cb753ed7…`), tx `9C503ED3F10F931A575ECA2A6048C8BD72EA600EBA023F8E82A2BB581BA4654D`
> - `send_receipt` (TC, 2 ids `d5e2ab02…`/`d039daa1…`): tx `FD720251DAA642AC7EE65C36BC7AFB977BD4C9729007D82204AA9AE23CBF67A3` (block 30021581) → receipt `5f67d0f7eec906e72bf724f1333b1657b6c924773ee88a6e33a62706a421158a`
> - receipt delivered on Solana: `ProcessedMessage` PDA `pFtaCoYr9UQaMLjVwD5SGp8KZeVDXnH8vqYxhDzmgZ6` exists → `handle` credited 2×499000 = 998000 lamports into `opsol(0)` (`8pz9ToVy…`)
> - operator withdrawal: `7mf9HE9Ck5fYqRg2XnLt9VoArFw3HBYUjhsZmsY2GLh5yk79mnDNy8XDaqsCdvQ18NiXwQFT8XYXLEGcMqUecU5`

## BSC→TC receipts: the deliverer is cosmjs, not the relayer (shared key)

Discovered on 08/21/2026: the account `terra1run9wz…` is signed by the RELAYER, the
claim-agent (emits a receipt every 5 min) and the scripts. The relayer CACHES the
sequence; when another signer uses the account, the relayer's sequence goes stale and
EVERY tx it sends on TC fails at CheckTx (`executed:false, gas_used:0` — no spend,
but no delivery). Since the key is the same by project decision, the relayer does NOT
deliver receipts on TC reliably.

Solution: `deploy/deliver-receipts-tc.mjs` (cosmjs, fetches the FRESH sequence on each
signature → immune) is the PRIMARY deliverer of the BSC→TC receipts — 3 min timer,
STUCK_MINUTES=3. The relayer keeps trying (harmless failure, 0 gas) and remains
primary for TC→remote (transfers, which sign on the remote chains, without
contention). BSC→TC commissions land in ~3-6 min.
