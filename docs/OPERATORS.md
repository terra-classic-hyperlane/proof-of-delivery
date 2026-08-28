# Operators and Quorum — how to query, configure, and change

Operational guide: who is in the quorum, how to register today's 2 operators,
and how to **add/remove** them as the set grows or shrinks.

## Concepts in 30 seconds

- **Operator** = the wallet the relayer uses ON THAT chain (addresses differ
  per chain: `terra1…` on TC, `0x…` on the EVMs, base58 pubkey on Solana).
  Register on each chain the address for that chain.
- **Quorum** = how many submissions/approvals from DISTINCT operators are
  required for the contract to act (apply a price on the oracle; on Solana, also
  close a credit epoch and execute vault proposals).
- The list lives **in the contract state** — only the **owner** (governance on TC,
  multisig on the remotes) changes operators/quorum. Automatic invariants:
  `1 ≤ quorum ≤ number of operators` and the list is never empty (the tx reverts).
- Rule for the current 2 operators: with a **2-of-2** quorum, one operator offline
  BLOCKS the price update (the bounds still protect and the owner emergency path
  remains available). When the 3rd operator joins, switch to **2-of-3**.

---

## Terra Classic (oracle-governor CosmWasm)

**Query who is in the quorum:**
```bash
terrad q wasm contract-state smart <GOVERNOR> '{"operators":{}}' --node $NODE
terrad q wasm contract-state smart <GOVERNOR> '{"config":{}}'    --node $NODE   # includes quorum
```

**Register at deploy time** (the script already supports it):
```bash
OPERATOR2=terra1... QUORUM=2 bash deploy/tc-deploy.sh
```

**Add / remove later** (the OWNER signs — today your wallet; later, a governance proposal):
```bash
# add the 3rd and raise to 2-of-3:
terrad tx wasm execute <GOVERNOR> '{"set_operators":{"add":["terra1NEW"],"remove":[]}}' $TXFLAGS
terrad tx wasm execute <GOVERNOR> '{"set_quorum":{"quorum":2}}' $TXFLAGS

# remove one (lower the quorum FIRST if needed — the contract forbids quorum > operators):
terrad tx wasm execute <GOVERNOR> '{"set_quorum":{"quorum":1}}' $TXFLAGS
terrad tx wasm execute <GOVERNOR> '{"set_operators":{"add":[],"remove":["terra1LEFT"]}}' $TXFLAGS
```

> The TC **vault** has no operator list on purpose: `Claim` is open
> and pays only whoever the Mailbox records as the executor of the delivery.

---

## BSC / Ethereum (GasOracleGovernor.sol)

**Query** (the EVM keeps a mapping, not a list — check by address or via the
`OperatorAdded/OperatorRemoved` events):
```bash
cast call $GOVERNOR "isOperator(address)(bool)" 0xOPERATOR --rpc-url $RPC
cast call $GOVERNOR "operatorCount()(uint256)" --rpc-url $RPC
cast call $GOVERNOR "quorum()(uint256)"        --rpc-url $RPC
cast logs --rpc-url $RPC --address $GOVERNOR "OperatorAdded(address)"   # history
```

**Register at deploy time:** `OPERATOR2=0x... QUORUM=2 PRIVATE_KEY=0x... bash deploy/evm-deploy.sh bsc`

**Add / remove later** (the governor OWNER signs):
```bash
cast send $GOVERNOR "setOperators(address[],address[])" "[0xNEW]" "[]" --private-key $PK --rpc-url $RPC
cast send $GOVERNOR "setQuorum(uint256)" 2 --private-key $PK --rpc-url $RPC
# remove: setOperators("[]","[0xLEFT]") — adjust the quorum first, if needed
```

---

## Solana

There are **two independent lists** (governor and vault):

### igp-oracle-governor — the MULTISIG changes it (1 signature)
Instruction `SetOperators { add, remove }` (variant 3) and `SetQuorum` (variant 4).
Query: read the config PDA (`["gov","-","config"]`) — the operators are in the
`Config` struct. E.g.: `solana account <CONFIG_PDA> --output json` + borsh decode
(`deploy/solana-init.mjs` prints the PDA at deploy time).

### relayer-reward-vault (rrv) — changed by a QUORUM PROPOSAL (no single admin)
Each operator sends the SAME envelope; it executes upon reaching the quorum:
```
SubmitAdminAction { envelope: { nonce: N, action: AddOperator(<pubkey>) } }
SubmitAdminAction { envelope: { nonce: N, action: SetQuorum(2) } }
SubmitAdminAction { envelope: { nonce: N, action: RemoveOperator(<pubkey>) } }
```
- The `nonce` allows repeating the same action in the future without colliding with an already-executed proposal;
- The proposal PDA is derived from the hash of the envelope — everyone converges on the same
  account without coordinating anything;
- With the current 2-of-2 quorum, **both** must submit for it to execute.

**Register at deploy time:** `OPERATOR2=<pubkey> bash deploy/solana-deploy.sh`
(the init creates governor and rrv already with the 2 operators and quorum 2).

---

## Checklist when changing the operator set

1. Register the correct address **on each chain** (4 lists: TC governor, BSC, ETH, SOL governor + SOL rrv);
2. Adjust the quorum along with it (recommendation: quorum = simple majority, e.g.: 2-of-3);
3. The new operator configures their own **oracle-agent** (their own keys!) and the relayer;
4. Never share a key between operators — it collapses the quorum into 1 entity;
5. Record the change in the public proposal/announcement (auditability).
