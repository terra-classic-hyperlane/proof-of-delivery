# Contract Operation Manual (4 networks)

How to execute each contract function in production: change owner, operators,
quorum, bounds, price, pause, withdrawals. Addresses: `REGISTRO-AUDITORIA.md`.
⚠️ Every owner change is **one-way once confirmed** — check the address 3× before.
Convention: `<...>` = value to fill in.

## 1. Terra Classic (terrad)

```bash
NODE=https://rpc.terra-classic.hexxagon.io
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
GOV=terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj
ORACLE=terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d
TX="--from operador --gas auto --gas-adjustment 1.4 --gas-prices 28.325uluna --node $NODE --chain-id columbus-5 -y"
```

### Vault (any relayer)
```bash
# claim the reward of YOUR deliveries (message_ids in hex, without 0x)
terrad tx wasm execute $VAULT '{"claim":{"message_ids":["<id_hex64>"]}}' $TX
# pull the accumulated IGP balance into the pool (permissionless)
terrad tx wasm execute $VAULT '{"sweep":{}}' $TX
```

### Vault (owner only)
```bash
# change OWNER (handoff to governance) / fee / window — optional fields
terrad tx wasm execute $VAULT '{"update_config":{"owner":"<terra1...>"}}' $TX
terrad tx wasm execute $VAULT '{"update_config":{"reward_per_delivery":"50000000","claim_window_blocks":200000}}' $TX
terrad tx wasm execute $VAULT '{"set_pause":{"paused":true}}' $TX
terrad tx wasm execute $VAULT '{"withdraw_surplus":{"to":"<terra1...>","amount":"1000000"}}' $TX
```

### Governor — price (operator) and administration (owner)
```bash
# operator submits price (applies on its own when the epoch quorum closes)
terrad tx wasm execute $GOV '{"submit_price":{"domain":56,"token_exchange_rate":"1098","gas_price":"3000000000"}}' $TX
# owner: operators / quorum / bounds / epoch / delta
terrad tx wasm execute $GOV '{"set_operators":{"add":["<terra1...>"],"remove":[]}}' $TX
terrad tx wasm execute $GOV '{"set_quorum":{"quorum":2}}' $TX
terrad tx wasm execute $GOV '{"set_bounds":{"domain":56,"bounds":{"min_rate":"366","max_rate":"3294","min_gas":"1000000000","max_gas":"9000000000"}}}' $TX
terrad tx wasm execute $GOV '{"set_owner":{"owner":"<terra1...governance>"}}' $TX      # handoff
# emergency (owner): forces price ignoring quorum/delta (within bounds)
terrad tx wasm execute $GOV '{"force_set_remote_gas_data":{"domain":56,"token_exchange_rate":"1098","gas_price":"3000000000"}}' $TX
# return ownership of the ORACLE (2 steps: governor initiates, new owner claims)
terrad tx wasm execute $GOV '{"init_oracle_ownership_transfer":{"next_owner":"<terra1...>"}}' $TX
terrad tx wasm execute $GOV '{"revoke_oracle_ownership_transfer":{}}' $TX   # changed your mind? revoke
```

### Queries
```bash
terrad q wasm contract-state smart $VAULT '{"solvency":{}}' --node $NODE
terrad q wasm contract-state smart $VAULT '{"layout_check":{"message_id":"<hex64>"}}' --node $NODE
terrad q wasm contract-state smart $GOV '{"operators":{}}' --node $NODE
terrad q wasm contract-state smart $GOV '{"submissions":{"domain":56,"epoch":<n>}}' --node $NODE
```

## 2. BSC and Ethereum (cast)

```bash
# BSC (use --legacy on EVERY BSC tx; on ETH, omit)
RPC=https://bsc-dataseed.bnbchain.org
VAULT=0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D
GOV=0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5
# ETH: RPC=https://ethereum-rpc.publicnode.com · VAULT=0xDf90d3b7FF98466E148B334128374807b3e89EbD · GOV=0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA
SEND="cast send --legacy --rpc-url $RPC --private-key $PK"
```

### Vault
```bash
$SEND $VAULT "claim(bytes32[])" "[<0xid1>,<0xid2>]"                  # relayer claims
$SEND $VAULT --value 5000000000000000                                 # seed the pool (anyone)
$SEND $VAULT "setParams(uint256,uint256)" <new_reward_wei> <window>   # owner
$SEND $VAULT "setPause(bool)" true                                    # owner
$SEND $VAULT "withdrawSurplus(address,uint256)" <to> <wei>            # owner
# OWNER change in 2 STEPS (only completes when the new one accepts):
$SEND $VAULT "transferOwnership(address)" <new_owner>                 # current owner
$SEND $VAULT "acceptOwnership()"                                      # NEW owner signs
```

### Governor
```bash
$SEND $GOV "submitPrice(uint32,uint128,uint128)" 132556 <rate> <gas>  # operator
$SEND $GOV "setOperators(address[],address[])" "[<add>]" "[]"         # owner
$SEND $GOV "setQuorum(uint256)" 2                                     # owner
$SEND $GOV "setBounds(uint32,(uint128,uint128,uint128,uint128,bool))" 132556 "(<minR>,<maxR>,<minG>,<maxG>,true)"
$SEND $GOV "setEpochDuration(uint256)" 21600
$SEND $GOV "setMaxDeltaBps(uint256)" 2000
$SEND $GOV "forceSetRemoteGasData(uint32,uint128,uint128)" 132556 <rate> <gas>  # emergency
$SEND $GOV "transferOracleOwnership(address)" <new_oracle_owner>      # returns the oracle
$SEND $GOV "transferOwnership(address)" <multisig> && (new owner) "acceptOwnership()"  # 2-step handoff
```

### Vault v2 — ClaimRemote (origin fee per remote delivery)
```bash
$SEND $VAULT "setRemoteOperators(address[],uint256)" "[<attester>]" 1          # owner
$SEND $VAULT "setRemoteBinding(address,uint32,string)" <operator> 132556 "terra1..."  # owner
$SEND $VAULT "setRemoteReward(uint32,uint256)" 132556 <wei>                    # owner (0 disables)
$SEND $VAULT "attestRemoteDelivery(uint32,bytes32[],address)" 132556 "[<0xid>]" 0x0000000000000000000000000000000000000000  # attester (agent does it on its own)
cast call $VAULT "remoteClaimed(bytes32)(address,uint32,uint256,uint256)" <0xid>      # audit
```

## 3. Solana (single `pod` program)

Every instruction data starts with **1 module byte**: `0x00`=vault(rrv) ·
`0x01`=governor; the rest is the module's borsh instruction. There is no ready-made CLI —
the scripts in this repo are the executable reference:

| Action | How |
|---|---|
| Submit price (operator) | `oracle-agent` (automatic) or gov instruction variant 1 |
| Operators add/remove (multisig) | `node deploy/register-solana-operator.mjs [pubkey]` (gov variant 3) |
| Quorum / epoch / delta (multisig) | gov variants 4 / 5 / 6 |
| Change multisig (handoff) | gov variant 7 `SetMultisig(pubkey)` |
| Emergency price (multisig) | gov variant 8 `ForceSetGasData` |
| IGP beneficiary (multisig) | gov variant 9 |
| **Return IGP ownership (emergency)** | gov variant 10 `TransferIgpOwnership(Some(pubkey))` |
| Epoch report / withdrawal (vault) | rrv variants 1 (`SubmitEpochReport`, now with field `remote` = origin credits) / 2 (`Withdraw`) |
| v2: remote reward/binding (proposal) | `AdminAction::SetRemoteReward` / `SetRemoteBinding` — model in `deploy/rrv-remote-config.mjs` |
| Seed the pool | transfer SOL to `Eq1mJGTS…Dwb9w` |
| Upgrade authority → multisig | `solana program set-upgrade-authority 2mQZcHYL… --new-upgrade-authority <MULTISIG>` |

Accounts per instruction: comments in `svm/programs/*/src/lib.rs` (enum
`Instruction`) and ready examples in `deploy/solana-init.mjs`.

## 4. Handoff sequence (end of deployment)

1. **TC** (current owner `terra1run9wz…`): `update_config{owner}` on the vault and
   `set_owner` on the governor → governance module `terra10d07y26…xf95n`.
2. **BSC/ETH**: `transferOwnership(multisig)` on the vault and on the governor + the
   multisig executes `acceptOwnership()` on each one (4 accepts in total).
3. **Solana**: gov `SetMultisig(<multisig>)` + `solana program set-upgrade-authority`.
4. Check everything with section 7 of `REGISTRO-AUDITORIA.md`.
