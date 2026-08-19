# Recibo Trustless — passo a passo dos comandos

Modelo sem confiança: o vault de DESTINO prova a entrega on-chain e despacha um
recibo assinado pelos validadores da ponte; o vault de ORIGEM paga ao receber.
Nenhum atestador, nenhum agente com poder de decisão — imune a relayer malicioso.

> **Status:** interface-alvo (contratos em construção). As assinaturas abaixo são
> o contrato que está sendo implementado; este doc e o código nascem alinhados.

## Conceitos

- **Mesmo contrato vault** em cada chain exerce DOIS papéis conforme a direção:
  - ORIGEM (msgs que saíram dela): guarda o pool · `handle()` recebe o recibo · paga.
  - DESTINO (msgs entregues nela): `send_receipt` prova a entrega e despacha o recibo.
- **Sem limiar on-chain** — o operador decide quando enviar (consulta `quote` antes).
- **Domínio de origem = prova** — lido da própria mensagem Hyperlane (não é palpite).
- **Vínculo no DESTINO** — `binding[executor_local][domínio_origem] → endereço de pagamento na origem`.
- **Operador paga o gás** do recibo (recupera na recompensa; por isso o batching).

Domínios: TC `132556` · BSC `56` · ETH `1` · Solana `1399811149`.

---

## A. Setup único (owner) — por corredor

Para o corredor X→Y (origem X paga; entrega em Y; recibo Y→X):

### A.1 No vault de ORIGEM (X) — quem paga
```bash
# router confiável: só aceita recibo vindo do vault de Y (allowlist do handle)
# TC:
terrad tx wasm execute $VAULT_X '{"set_remote_router":{"domain":<Y>,"router":"<vault_Y_hex32>"}}' $TX
# EVM:
cast send $VAULT_X "setRemoteRouter(uint32,bytes32)" <Y> <vault_Y_bytes32>
# recompensa por entrega no domínio Y (≈ taxa real de origem):
terrad tx wasm execute $VAULT_X '{"set_remote_reward":{"domain":<Y>,"reward":"<valor>"}}' $TX
cast send $VAULT_X "setRemoteReward(uint32,uint256)" <Y> <valor_wei>
```

### A.2 No vault de DESTINO (Y) — quem prova e envia
```bash
# router de volta: para onde despachar o recibo (o vault de X)
cast send $VAULT_Y "setRemoteRouter(uint32,bytes32)" <X> <vault_X_bytes32>
terrad tx wasm execute $VAULT_Y '{"set_remote_router":{"domain":<X>,"router":"<vault_X_hex32>"}}' $TX
# VÍNCULO de identidade: o executor Y → o endereço que recebe na origem X
cast send $VAULT_Y "setRemoteBinding(address,uint32,string)" <executor_em_Y> <X> "<endereco_pagamento_em_X>"
terrad tx wasm execute $VAULT_Y '{"set_remote_binding":{"operator":"<executor_em_Y>","domain":<X>,"remote_address":"<endereco_em_X>"}}' $TX
```

### A.3 Infra Hyperlane (rota do recibo)
Registrar o vault de X como **recipient** e garantir que o ISM de entrada de X
aceita mensagens do vault de Y. (Config de infra — não altera contratos Hyperlane.)

---

## B. Fluxo TC → BSC (origem TC paga em LUNC)

Você já despachou (ex.: `message_id` = `0x<id>`), seu relayer entregou na BSC.

### B.1 Consultar quanto vale (na ORIGEM, TC)
```bash
NODE=https://rpc.terra-classic.hexxagon.io
terrad q wasm contract-state smart $VAULT_TC \
  '{"quote_remote":{"domain":56,"message_ids":["<id_hex_sem_0x>"]}}' --node $NODE
# → { "amount": "<LUNC a receber>", "payable_count": <n> }
```
Decida: `amount` cobre o gás do recibo com folga? Se sim, siga. Se não, acumule
mais entregas e repita (batching — 1 recibo cobre N ids).

### B.2 Enviar o recibo (no DESTINO, BSC) — operador paga o gás
```bash
# msg.value cobre a cotação do IGP da BSC p/ entregar o recibo no TC
cast send --legacy $VAULT_BSC "sendReceipt(uint32,bytes32[])" 132556 "[0x<id>,0x<id2>]" \
  --value <gas_igp_wei> --private-key $PK --rpc-url https://bsc-dataseed.bnbchain.org
```
O vault da BSC: prova `processor(id)` de cada id → lê o domínio de origem da msg
(132556) → confere o vínculo → despacha o recibo para o router do TC.

### B.3 O relayer entrega o recibo no TC → pagamento automático
Nenhum comando: o vault do TC recebe via `handle`, confere o router da BSC, e
paga os LUNC ao endereço vinculado. Verificar:
```bash
terrad q wasm contract-state smart $VAULT_TC \
  '{"remote_claimed":{"message_id":"<id>"}}' --node $NODE
# → { "claimed": true, "executor": "terra1...", "amount": "...", ... }
```

---

## C. Fluxo BSC → TC (origem BSC paga em BNB)

Espelho do B. Você despachou BSC→TC; seu relayer entregou no TC.

### C.1 Consultar (na ORIGEM, BSC)
```bash
cast call $VAULT_BSC "quoteRemote(uint32,bytes32[])(uint256,uint256)" 132556 "[0x<id>]" \
  --rpc-url https://bsc-dataseed.bnbchain.org
# → (amount_wei, payableCount)
```

### C.2 Enviar o recibo (no DESTINO, TC) — operador paga o gás
```bash
terrad tx wasm execute $VAULT_TC \
  '{"send_receipt":{"domain":56,"message_ids":["<id>"]}}' \
  --amount <gas_igp>uluna $TX
```
O vault do TC prova a entrega (raw query DELIVERIES) → lê a origem (56) → vínculo
→ despacha o recibo para o router da BSC.

### C.3 Pagamento automático na BSC
```bash
cast call $VAULT_BSC "remoteClaimed(bytes32)(address,uint32,uint256,uint256)" 0x<id> \
  --rpc-url https://bsc-dataseed.bnbchain.org
# → executor 0x8f08…, domínio 132556, valor em wei > 0
```

---

## D. Frontend (futuro)

O front amarra os dois passos que ficam em chains diferentes:
1. lista as entregas do operador ainda não pagas (varre os Mailboxes);
2. chama `quote_remote` na ORIGEM e mostra o acumulado + o gás estimado;
3. um botão "Enviar recibo" que executa `send_receipt` no DESTINO;
4. acompanha o `remote_claimed` até o pagamento.

---

## E. Segurança (por que é trustless)

- O vault de origem só aceita `handle` do **Mailbox** e do **router registrado**
  (o vault de destino) — recibo forjado é rejeitado.
- O recibo só existe se a entrega foi **provada on-chain** no destino
  (`processor(id)`), e passou pela **validação dos validadores/ISM** na volta.
- O domínio de origem é **lido da mensagem** (comprometido pelo `message_id`) —
  não dá para desviar o pagamento para o pool de outra chain.
- **1 pagamento por id** (`remote_claimed`, effects-first) e teto por domínio.
- Modelo comparado (confiança × custo): `SEGURANCA-CLAIMREMOTO.md` §3.
