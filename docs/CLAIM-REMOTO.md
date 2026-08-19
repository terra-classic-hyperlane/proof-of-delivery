# ClaimRemote (Vault v2) — como as 4 chains se amarram

**Para quem opera relayer em TC + BSC + ETH + Solana.** A v2 faz o vault do TC
pagar, em LUNC, as entregas que os SEUS endereços fizeram nas chains remotas —
usando o fato de que **o message_id é o mesmo nas duas pontas** de cada mensagem.

## 1. A amarração (o mapa de identidade)

Um único operador possui endereços diferentes em cada chain. O vault do TC
guarda esse mapa (`SetRemoteBinding`, editável só pelo owner — depois, governança):

```
                         VAULT v2 no Terra Classic
                terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
                                     │
     operador terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp (você)
                                     │ vínculos por domínio:
        ┌────────────────────────────┼─────────────────────────────┐
        ▼                            ▼                             ▼
  domain 56 (BSC)             domain 1 (ETH)            domain 1399811149 (Solana)
  0x8f085bad…5291             0xef818120…00ae           PbEo7Fn2…cwwrkS
  (relayer BSC)               (relayer ETH)             (relayer Solana)
```

## 2. O ciclo completo de uma mensagem TC → remota

```
1. usuário envia IGORFAKE do TC p/ BSC
   └─ paga a taxa em LUNC → IGP → (Sweep automático) → POOL do vault
2. SEU relayer BSC (0x8f08…) executa o process() no Mailbox da BSC
   └─ o message_id da entrega = o MESMO do dispatch no TC
3. o claim-agent (off-chain, na VPS) VERIFICA a entrega na BSC
   └─ evento ProcessId no Mailbox + mailbox.processor(id) == 0x8f08…
4. o claim-agent ATESTA no vault do TC:
   AttestRemoteDelivery { domain: 56, message_ids: [id] }
5. o vault confere: atestador registrado ✓ · vínculo (você, 56) = 0x8f08… ✓
   · id nunca pago ✓ · quórum atingido ✓ → PAGA a recompensa do domínio
   em LUNC para terra1run9wz…  ←  a taxa de origem voltou para o operador
```

O mesmo vale para ETH e Solana — só muda o domínio e o endereço vinculado.
(E a entrega NO PRÓPRIO TC continua com o `Claim` clássico, por prova direta.)

## 3. As duas fontes de renda do operador, lado a lado

| Entrega feita em | Quem paga | Quanto | Prova |
|---|---|---|---|
| TC (rota de entrada) | pool TC via `Claim` | 50 LUNC | **direta** (raw query DELIVERIES) |
| BSC | pool BSC via `claim()` | 0,00005 BNB | direta (processedAt) |
| ETH | pool ETH via `claim()` | 0,0004 ETH | direta (processedAt) |
| Solana | pool SOL via época | 0,003 SOL | quórum de operadores |
| **BSC/ETH/SOL (v2)** | **pool TC via `AttestRemoteDelivery`** | **33 LUNC/entrega (por domínio, owner define)** | **atestação com quórum + vínculo** |

## 4. O modelo de confiança (honesto)

O TC **não enxerga** as outras chains. A v2 não muda isso — ela replica o
modelo já aprovado para o vault da Solana (que também não registra executor):
**quórum de atestadores registrados**. As amarras que limitam abuso:

1. **Vínculo prévio**: só paga a endereço remoto que o owner/governança vinculou;
2. **1 pagamento por message_id** (`REMOTE_CLAIMED`, effects-first) — id nunca paga 2×;
3. **Recompensa fixa por domínio** — um id falso custa no máximo 33 LUNC, nunca o pool;
4. **Quórum de atestações CONCORDANTES** (mesmo executor). Hoje = 1
   (auto-atestação — aceitável porque owner = operador único = você, em teste).
   **Com 2+ operadores independentes, subir para ≥ 2** (`SetRemoteOperators`);
5. **Auditoria pública**: `RemoteAttestations{message_id}` mostra quem atestou o
   quê; qualquer um confere o message_id nas duas chains (é o mesmo hash);
6. `SetPause` congela tudo em emergência.

## 5. Operação (comandos)

```bash
V=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
TX="--from operador --gas auto --gas-adjustment 1.4 --gas-prices 28.325uluna --chain-id columbus-5 --node https://rpc.terra-classic.hexxagon.io -y"

# owner: atestadores + quórum · vínculos · recompensa por domínio
terrad tx wasm execute $V '{"set_remote_operators":{"attestors":["terra1..."],"quorum":1}}' $TX
terrad tx wasm execute $V '{"set_remote_binding":{"operator":"terra1...","domain":56,"remote_address":"0x..."}}' $TX
terrad tx wasm execute $V '{"set_remote_reward":{"domain":56,"reward":"33000000"}}' $TX

# atestador: atesta entregas (o claim-agent faz isso sozinho)
terrad tx wasm execute $V '{"attest_remote_delivery":{"domain":56,"message_ids":["<id_hex64>"]}}' $TX

# consultas de auditoria
terrad q wasm contract-state smart $V '{"remote_config":{}}' --node <NODE>
terrad q wasm contract-state smart $V '{"remote_claimed":{"message_id":"<id>"}}' --node <NODE>
terrad q wasm contract-state smart $V '{"remote_attestations":{"message_id":"<id>"}}' --node <NODE>
```

## 6. Automação (claim-agent)

O claim-agent já verifica as entregas nas 4 chains; com a v2 ele ganhou o passo
final: toda entrega REMOTA confirmada como sua entra numa fila
(`state.json → remoteAttest`) e é atestada no vault do TC na mesma rodada
horária — log: `✓ atestado dom <n> → <tx>`. Nenhuma ação manual.

## 7. Implantação da v2

Build reproduzível `cosmwasm/optimizer:0.17.0` → `relayer_reward_vault.wasm`
sha256 `e24a5e66ab4a503c6acf369710b717310362d2ae5fa7b9800542c8272b2fc801`.
Migração **no mesmo endereço** EXECUTADA em 19/08/2026 (code_id **11589**,
store `A9866AEE…`, migrate `C4075BA8…`) via `deploy/tc-migrate-vault-v2.sh`
(LOCAL — regra do projeto: nada de wasm/deploy na VPS). Primeiros pagamentos:
99 LUNC pelas 3 entregas do dia (txs em `AUDITORIA-TC.md`).
30 testes verdes (5 novos da v2: quórum 1 paga, anti-duplo, quórum 2 espera
concordância, rejeições, totais). Registro de execução: `AUDITORIA-TC.md`.

## 8. v2 também na BSC e na ETH (espelho do modelo)

Os vaults EVM ganharam o MESMO módulo (`attestRemoteDelivery` etc., 38 testes
foundry). Como os contratos EVM não são migráveis e os pools estavam zerados,
a v2 é um **deploy novo** + `igp.setBeneficiary(v2)` — script LOCAL
`deploy/evm-vault-v2.sh bsc|ethereum`, que também configura: atestador = owner,
quórum 1, vínculo `(owner, 132556) → terra1run9wz…` e **recompensa = cotação
real do IGP** (`quoteGasPayment(132556, destinationGas)`) — exatamente a taxa
que o usuário paga na origem.

Fluxo espelhado: usuário despacha DA BSC → paga taxa em BNB → seu relayer
entrega NO TC → claim-agent detecta a entrega no TC (o evento traz a ORIGEM),
enfileira e atesta no vault da BSC → **a taxa em BNB volta para o operador**.
O mesmo para a ETH. Assim, TODAS as 4 chains pagam a taxa de origem ao executor.

## 9. v2 na Solana (via relatório de época)

Na Solana, PDA por mensagem custaria mais rent (~0,0015 SOL) que a própria taxa
(0,000499 SOL). Por isso o `EpochReport` ganhou o campo `remote:
[(domínio, operador, contagem)]` — os créditos remotos passam pelo MESMO
hash/quórum do relatório e saem pelo `Withdraw` normal, custo extra zero.
Config por proposta administrativa: reward `499.000 lamports` (taxa real medida)
e vínculo `(132556, PbEo7Fn2…) → terra1run9wz…`. O claim-agent agrega as
entregas de msgs Solana→TC por época e as inclui no relatório automaticamente.

## 10. Sustentabilidade (atenção do owner/governança)

A v2 paga do MESMO pool que o `Claim` local. Com recompensa 33 LUNC ≈ taxa média
de origem, o pool fica ~neutro (entra taxa, sai recompensa). Se a governança
subir muito a recompensa remota, o pool drena — monitorar `total_remote_paid`
vs arrecadação do Sweep (`RemoteConfig{}` + `Solvency{}`).
