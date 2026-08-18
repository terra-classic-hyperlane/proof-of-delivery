# Manual de Operação dos Contratos (4 redes)

Como executar cada função dos contratos em produção: trocar owner, operadores,
quórum, faixas, preço, pausa, saques. Endereços: `REGISTRO-AUDITORIA.md`.
⚠️ Toda mudança de owner é **de mão única depois de confirmada** — confira o
endereço 3× antes. Convenção: `<...>` = valor a preencher.

## 1. Terra Classic (terrad)

```bash
NODE=https://rpc.terra-classic.hexxagon.io
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
GOV=terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj
ORACLE=terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d
TX="--from operador --gas auto --gas-adjustment 1.4 --gas-prices 28.325uluna --node $NODE --chain-id columbus-5 -y"
```

### Vault (qualquer relayer)
```bash
# sacar recompensa das SUAS entregas (message_ids em hex, sem 0x)
terrad tx wasm execute $VAULT '{"claim":{"message_ids":["<id_hex64>"]}}' $TX
# puxar o saldo acumulado do IGP p/ o pool (permissionless)
terrad tx wasm execute $VAULT '{"sweep":{}}' $TX
```

### Vault (só o owner)
```bash
# trocar OWNER (handoff p/ a governança) / tarifa / janela — campos opcionais
terrad tx wasm execute $VAULT '{"update_config":{"owner":"<terra1...>"}}' $TX
terrad tx wasm execute $VAULT '{"update_config":{"reward_per_delivery":"50000000","claim_window_blocks":200000}}' $TX
terrad tx wasm execute $VAULT '{"set_pause":{"paused":true}}' $TX
terrad tx wasm execute $VAULT '{"withdraw_surplus":{"to":"<terra1...>","amount":"1000000"}}' $TX
```

### Governor — preço (operador) e administração (owner)
```bash
# operador submete preço (aplica sozinho quando o quórum da época fecha)
terrad tx wasm execute $GOV '{"submit_price":{"domain":56,"token_exchange_rate":"1098","gas_price":"3000000000"}}' $TX
# owner: operadores / quórum / faixa / época / delta
terrad tx wasm execute $GOV '{"set_operators":{"add":["<terra1...>"],"remove":[]}}' $TX
terrad tx wasm execute $GOV '{"set_quorum":{"quorum":2}}' $TX
terrad tx wasm execute $GOV '{"set_bounds":{"domain":56,"bounds":{"min_rate":"366","max_rate":"3294","min_gas":"1000000000","max_gas":"9000000000"}}}' $TX
terrad tx wasm execute $GOV '{"set_owner":{"owner":"<terra1...governança>"}}' $TX      # handoff
# emergência (owner): força preço ignorando quórum/delta (dentro da faixa)
terrad tx wasm execute $GOV '{"force_set_remote_gas_data":{"domain":56,"token_exchange_rate":"1098","gas_price":"3000000000"}}' $TX
# devolver a posse do ORACLE (2 passos: governor inicia, novo dono reivindica)
terrad tx wasm execute $GOV '{"init_oracle_ownership_transfer":{"next_owner":"<terra1...>"}}' $TX
terrad tx wasm execute $GOV '{"revoke_oracle_ownership_transfer":{}}' $TX   # arrependeu? revoga
```

### Consultas
```bash
terrad q wasm contract-state smart $VAULT '{"solvency":{}}' --node $NODE
terrad q wasm contract-state smart $VAULT '{"layout_check":{"message_id":"<hex64>"}}' --node $NODE
terrad q wasm contract-state smart $GOV '{"operators":{}}' --node $NODE
terrad q wasm contract-state smart $GOV '{"submissions":{"domain":56,"epoch":<n>}}' --node $NODE
```

## 2. BSC e Ethereum (cast)

```bash
# BSC (use --legacy em TODA tx BSC; na ETH, omita)
RPC=https://bsc-dataseed.bnbchain.org
VAULT=0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D
GOV=0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5
# ETH: RPC=https://ethereum-rpc.publicnode.com · VAULT=0xDf90d3b7FF98466E148B334128374807b3e89EbD · GOV=0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA
SEND="cast send --legacy --rpc-url $RPC --private-key $PK"
```

### Vault
```bash
$SEND $VAULT "claim(bytes32[])" "[<0xid1>,<0xid2>]"                  # relayer saca
$SEND $VAULT --value 5000000000000000                                 # semear o pool (qualquer um)
$SEND $VAULT "setParams(uint256,uint256)" <novo_reward_wei> <janela>  # owner
$SEND $VAULT "setPause(bool)" true                                    # owner
$SEND $VAULT "withdrawSurplus(address,uint256)" <para> <wei>          # owner
# troca de OWNER em 2 PASSOS (só completa quando o novo aceita):
$SEND $VAULT "transferOwnership(address)" <novo_owner>                # owner atual
$SEND $VAULT "acceptOwnership()"                                      # NOVO owner assina
```

### Governor
```bash
$SEND $GOV "submitPrice(uint32,uint128,uint128)" 132556 <rate> <gas>  # operador
$SEND $GOV "setOperators(address[],address[])" "[<add>]" "[]"         # owner
$SEND $GOV "setQuorum(uint256)" 2                                     # owner
$SEND $GOV "setBounds(uint32,(uint128,uint128,uint128,uint128,bool))" 132556 "(<minR>,<maxR>,<minG>,<maxG>,true)"
$SEND $GOV "setEpochDuration(uint256)" 21600
$SEND $GOV "setMaxDeltaBps(uint256)" 2000
$SEND $GOV "forceSetRemoteGasData(uint32,uint128,uint128)" 132556 <rate> <gas>  # emergência
$SEND $GOV "transferOracleOwnership(address)" <novo_dono_do_oracle>   # devolve o oracle
$SEND $GOV "transferOwnership(address)" <multisig> && (novo owner) "acceptOwnership()"  # handoff 2 passos
```

## 3. Solana (programa único `pod`)

Todo instruction data começa com **1 byte de módulo**: `0x00`=vault(rrv) ·
`0x01`=governor; o resto é a instrução borsh do módulo. Não há CLI pronta —
os scripts deste repo são a referência executável:

| Ação | Como |
|---|---|
| Submeter preço (operador) | `oracle-agent` (automático) ou instrução gov variante 1 |
| Operadores add/remove (multisig) | `node deploy/register-solana-operator.mjs [pubkey]` (gov variante 3) |
| Quórum / época / delta (multisig) | gov variantes 4 / 5 / 6 |
| Trocar multisig (handoff) | gov variante 7 `SetMultisig(pubkey)` |
| Preço de emergência (multisig) | gov variante 8 `ForceSetGasData` |
| Beneficiary do IGP (multisig) | gov variante 9 |
| **Devolver posse do IGP (emergência)** | gov variante 10 `TransferIgpOwnership(Some(pubkey))` |
| Relatório de época / saque (vault) | rrv variantes 1 (`SubmitEpochReport`) / 2 (`Withdraw`) |
| Semear o pool | transferir SOL p/ `Eq1mJGTS…Dwb9w` |
| Upgrade authority → multisig | `solana program set-upgrade-authority 2mQZcHYL… --new-upgrade-authority <MULTISIG>` |

Contas por instrução: comentários em `svm/programs/*/src/lib.rs` (enum
`Instruction`) e exemplos prontos em `deploy/solana-init.mjs`.

## 4. Sequência de handoff (fim da implantação)

1. **TC** (owner atual `terra1run9wz…`): `update_config{owner}` no vault e
   `set_owner` no governor → módulo de governança `terra10d07y26…xf95n`.
2. **BSC/ETH**: `transferOwnership(multisig)` no vault e no governor + o
   multisig executa `acceptOwnership()` em cada um (4 aceites no total).
3. **Solana**: gov `SetMultisig(<multisig>)` + `solana program set-upgrade-authority`.
4. Conferir tudo com a seção 7 de `REGISTRO-AUDITORIA.md`.
