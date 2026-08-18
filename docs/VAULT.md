# Vault — como consultar, configurar e operar

Guia operacional do RelayerRewardVault nas 4 redes: o que o **owner** pode
mudar, como o **relayer** saca, e as consultas de monitoramento.

## O que é configurável (e por quem)

| Parâmetro | O que controla | Quem muda |
|---|---|---|
| `reward_per_delivery` | tarifa fixa paga por entrega comprovada | owner (TC/EVM) · quórum (SOL) |
| `claim_window_blocks` | prazo p/ resgatar após a entrega | idem |
| `paused` | bloqueia claims (emergência) | idem |
| `owner` | quem administra (→ governança/multisig no handoff) | owner atual |
| `mailbox` / `igp` (só TC) | fontes da prova e do Sweep | owner |
| retirada de excedente | saque do pool p/ um destino | owner (TC/EVM) · quórum com destino no hash (SOL) |

O que **ninguém** configura: quem pode sacar recompensa — o `Claim` paga
exclusivamente quem o registro da chain aponta como executor da entrega.

---

## Terra Classic

**Consultar:**
```bash
terrad q wasm contract-state smart <VAULT> '{"config":{}}'    --node $NODE   # tudo, incl. total pago
terrad q wasm contract-state smart <VAULT> '{"solvency":{}}'  --node $NODE   # pool e quantas entregas banca
terrad q wasm contract-state smart <VAULT> '{"claimed":{"message_id":"<hex64>"}}' --node $NODE
terrad q wasm contract-state smart <VAULT> '{"layout_check":{"message_id":"<id entregue>"}}' --node $NODE  # alarme de migrate
```

**Configurar (owner assina — hoje o deployer; depois, proposta de governança):**
```bash
# tarifa e/ou janela (campos são opcionais — mande só o que muda):
terrad tx wasm execute <VAULT> '{"update_config":{"reward_per_delivery":"75000000","claim_window_blocks":300000}}' $TXFLAGS
# pausar / despausar:
terrad tx wasm execute <VAULT> '{"set_pause":{"paused":true}}' $TXFLAGS
# retirar excedente:
terrad tx wasm execute <VAULT> '{"withdraw_surplus":{"to":"terra1...","amount":"1000000000"}}' $TXFLAGS
# handoff do owner (governança):
terrad tx wasm execute <VAULT> '{"update_config":{"owner":"<MODULO_GOV>"}}' $TXFLAGS
```

**Uso pelo relayer (permissionless):**
```bash
# puxa a arrecadação do IGP p/ o pool e resgata na MESMA tx (lote atômico):
terrad tx wasm execute <VAULT> '{"sweep":{}}' $TXFLAGS
terrad tx wasm execute <VAULT> '{"claim":{"message_ids":["<hex64>","<hex64>"]}}' $TXFLAGS
```

**Abastecer o pool:** qualquer `bank send` de uluna para o endereço do vault.

---

## BSC / Ethereum

**Consultar:**
```bash
cast call $VAULT "rewardPerDelivery()(uint256)" --rpc-url $RPC
cast call $VAULT "claimWindowBlocks()(uint256)" --rpc-url $RPC
cast call $VAULT "paused()(bool)"               --rpc-url $RPC
cast call $VAULT "claimsPayable()(uint256)"     --rpc-url $RPC   # solvência
cast call $VAULT "claimedBy(bytes32)(address)"  0x<id> --rpc-url $RPC
cast call $VAULT "totalPaid()(uint256)"         --rpc-url $RPC
```

**Configurar (owner assina):**
```bash
cast send $VAULT "setParams(uint256,uint256)" <REWARD_WEI> <WINDOW> --private-key $PK --rpc-url $RPC
cast send $VAULT "setPause(bool)" true --private-key $PK --rpc-url $RPC
cast send $VAULT "withdrawSurplus(address,uint256)" 0xDEST <WEI> --private-key $PK --rpc-url $RPC
# handoff (2 passos — o multisig precisa ACEITAR):
cast send $VAULT "transferOwnership(address)" 0xMULTISIG --private-key $PK --rpc-url $RPC
# ... e o multisig executa: acceptOwnership()
```

**Uso pelo relayer:** `igp.claim()` (permissionless, empurra a arrecadação ao
vault) e `vault.claim(bytes32[] ids)` — pode ir na mesma tx via multicall próprio.

**Abastecer o pool:** transferir BNB/ETH direto ao vault (`receive()` aceita).

---

## Solana (rrv)

Aqui NÃO há owner único: mudanças são **propostas com quórum de operadores**
(`AdminEnvelope { nonce, action }` — ver `docs/OPERADORES.md` §Solana):

| Ação | Envelope |
|---|---|
| tarifa | `SetRewardLamports(u64)` |
| pausa | `SetPaused(bool)` |
| duração da época | `SetEpochDuration(u64)` |
| operadores/quórum | `AddOperator/RemoveOperator/SetQuorum` |
| excedente | `WithdrawSurplus { to, amount }` — o **destino faz parte do hash**: aprova-se AQUELE destino |

**Consultar:** ler a config PDA `["rrv","-","config"]` (o init imprime o
endereço) — o saldo de lamports da PDA acima do rent-exempt É o pool. Créditos
por operador: PDA `["rrv","-","credit","-",<operador>]`.

**Uso pelo operador:** `SubmitEpochReport` (relatório da época, quórum de
hashes idênticos credita) e `Withdraw { amount }` (débito direto do pool,
limitado ao próprio crédito e ao rent-exempt).

**Abastecer o pool:** transfer de SOL para a config PDA (e registrar a PDA como
beneficiary do IGP — feito no `finalize` do deploy).

---

## Monitoramento mínimo (alarmes)

| Sinal | Onde | Ação |
|---|---|---|
| `layout_check.ok = false` (TC) | query no vault | migrate no Mailbox — **pausar** e investigar |
| `claims_payable` caindo < backlog | Solvency / claimsPayable | Sweep/claim do IGP não está rodando, ou tarifa > arrecadação |
| claims revertendo `NotProcessor` | logs do relayer | relayer usando carteira errada p/ o claim |
| `ClaimWindowExpired` frequente | logs | relayer resgatando tarde — automatizar claim pós-entrega |
