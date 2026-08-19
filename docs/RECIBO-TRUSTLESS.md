# Recibo Trustless — passo a passo dos comandos

Modelo sem confiança: o vault de DESTINO prova a entrega on-chain e despacha um
recibo assinado pelos validadores da ponte; o vault de ORIGEM paga ao receber.
Nenhum atestador, nenhum agente com poder de decisão — imune a relayer malicioso.

> **Status: PROVADO EM PRODUÇÃO (19/08/2026), corredor TC↔BSC nos 2 sentidos.**
> Vaults recibo: TC `terra1gqkrh2…` (code_id 11592) · BSC
> `0x34E06a7793877EC5251b1dC230aD7cD577d231f4` (ism = ISM do warp `0xa82087B8…`).
> Provas: BSC→TC pagou 33 LUNC (tx `F4700EF4…`, msg `974a7e47…`); TC→BSC pagou
> 2.259.538.750.000 wei BNB (msg `5920d3fb…`, recibo `b6d00d74…`). Detalhes de
> integração Hyperlane no §F.
>
> **Solana:** corredor **Solana→TC** PROVADO EM PRODUÇÃO (2026-08-19), sem keeper
> (relayer nativo) — desenho, formatos, passo a passo e provas no **§G**.
> TC→Solana fica fora (exigiria relayer customizado).

## Plano de implementação (fases)

1. **Registro de/para global** (Fase 1, EM ANDAMENTO) — cada vault guarda
   `operador N → {endereço por domínio}` (só o owner grava) + reverse-lookup
   `endereço → N`. Consolida os vínculos por-corredor num só registro de identidade.
2. **`send_receipt`** (papel DESTINO) — prova a entrega (`processor(id)`), lê o
   domínio de origem da MENSAGEM (comprometido pelo `message_id`), resolve o
   operador N do executor e despacha o recibo pelo Mailbox. Operador paga o gás.
3. **`handle`** (papel ORIGEM) — aceita SÓ do Mailbox + router registrado; para
   cada `(id, N)` paga o endereço de N no PRÓPRIO registro local (nunca um
   endereço vindo no recibo); 1× por id.
4. **Roteamento Hyperlane** do corredor TC↔BSC (config de infra, sem tocar em
   contrato nativo).
5. **Teste TC→BSC** ponta a ponta → depois **BSC→TC**.
6. **Replicar** ETH e Solana. ETH: mesmo contrato (adiado por gás). **Solana:
   só o sentido Solana→TC** é possível sem keeper — ver §G (o sentido TC→Solana
   exige keeper e foi descartado, pois a chain não grava o executor).

### Por que "recibo por ÍNDICE do operador" (não por endereço)

O `message_id` identifica a MENSAGEM, não o executor — quem entregou só é
registrado no DESTINO (`processor(id)`). O recibo carrega **(message_id, N)**;
a origem paga o **endereço de N no seu próprio registro** (definido pelo owner),
então nem um recibo malformado desvia o pagamento. O de/para é a espinha dorsal
de identidade, replicada em cada chain.

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

## A0. Deploy + config do corredor TC↔BSC (scripts prontos)

Ordem de execução (LOCAL — nunca na VPS):
```bash
# 1) deploy do vault-recibo da BSC + config do lado BSC (usa o TC vault como constante)
PRIVATE_KEY=0x<chave_bsc> bash deploy/evm-vault-receipt.sh bsc
#    → imprime BSC_VAULT=0x…  (novo endereço)

# 2) migrate do vault do TC (mesmo endereço, pool preservado) + config do lado TC
BSC_VAULT=0x<do_passo_1> bash deploy/tc-migrate-vault-receipt.sh
#    → pede a senha do keyring (chave hyperlane-deploy)

# 3) semear o pool da BSC (o do TC já tem 5.000 LUNC); qualquer valor:
cast send --legacy 0x<BSC_VAULT> --value 5000000000000000 --private-key 0x<bsc> --rpc-url https://bsc-dataseed.bnbchain.org
```
Cada script é idempotente (retoma pelo `.state`) e termina com a verificação
on-chain dos routers/rewards/registro. A rota Hyperlane do recibo usa o ISM
default (o corredor TC↔BSC já é validado nos 2 sentidos pelo warp).

Config manual equivalente (referência) abaixo.

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

---

## F. Integração Hyperlane — 2 detalhes que só a chain real revelou (19/08)

O vault é um **recipient** Hyperlane. Ao entregar o recibo, o Mailbox exige duas
coisas do recipient que os mocks de teste não cobriam:

1. **Responder a query de ISM.** O Mailbox pergunta `InterchainSecurityModule`
   ao recipient. Sem a query, `process()` reverte ("Error fetching ISM address").
   - CW: adicionada a variante `QueryMsg::IsmSpecifier(...)` → retorna `{ism:None}`.
   - EVM: `interchainSecurityModule()` já existia.
2. **Apontar para um ISM que conheça a ORIGEM do recibo.** `ism = None`/`address(0)`
   usa o ISM DEFAULT da chain — que pode não conhecer a origem. No TC o default
   já conhece a BSC (56); na BSC o default NÃO conhece o TC (132556) → erro
   `No ISM found for origin: 132556`. Solução: apontar para o **mesmo ISM do
   warp sintético** daquela rota (BSC: `0xa82087B8…`; ETH: `0xDe8edEC7…`), que já
   valida as mensagens vindas do TC. EVM: `setIsm(<ism_do_warp>)` (owner).

Regra geral p/ um corredor novo: o vault de CADA chain que RECEBE recibos aponta
`ism` para o ISM do warp que valida a origem dos recibos (= os validadores da
chain de origem). Corredor com warp bidirecional → esse ISM já existe.

Provado em produção 19/08: BSC→TC (recibo → TC, ISM default do TC) e TC→BSC
(recibo → BSC, `ism` = ISM do warp `0xa82087B8`).

---

## G. Solana — corredor **Solana→TC** sem keeper

### Por que só um sentido
O Mailbox Sealevel da Solana **não grava quem entregou** (o `struct ProcessedMessage`
em `mailbox/src/accounts.rs` só tem `discriminator, sequence, message_id, slot` — sem
executor). Logo:

| Sentido | Entrega em | A chain grava o executor? | Sem keeper + trustless? |
|---|---|---|---|
| **Solana→TC** | TC (grava `DELIVERIES.sender`) | ✅ | ✅ — igual ao BSC |
| **TC→Solana** | Solana (não grava) | ❌ | ❌ — exigiria keeper (relayer customizado) → **descartado** |

Como num projeto Terra Classic **não se roda relayer customizado**, o TC→Solana fica
fora. O Solana→TC usa **só o relayer nativo** e é trustless.

### Duas travas da Solana que mudaram o desenho (vs. EVM/CW)
O `handle` do `pod`, quando o Mailbox nativo o chama, **não recebe um payer** (o
Mailbox só antepõe o `process_authority` — ver `processor.rs`, o CPI ao recipient
não repassa a conta 0). Consequências:

1. **Não pode criar conta** (sem quem pague o rent) → o pagamento vai para a **PDA
   `operator_sol(index)`** (o índice vem no corpo do recibo, então é derivável ao
   simular o `HandleAccountMetas`). O operador saca depois com `WithdrawOperatorSol`.
2. **Não pode deduplicar por id** (idem) → a idempotência mora no **`send_receipt`
   do TC** (`SENT_RECEIPT[id]`): o destino não reemite recibo de um id já enviado.
   Somado à garantia do Mailbox (entrega única por mensagem), não há duplo-pagamento.

### O `pod` como recipient Hyperlane (formatos exatos da fonte do monorepo)
- `ism_response()` → `borsh(Some(WARP_ISM))` (33 bytes); o Mailbox lê como
  `Option::<Pubkey>::try_from_slice` (`processor.rs`).
- `ism_account_metas()` → `SimulationReturnData(vec![])` (nosso ISM é constante).
- `handle_account_metas()` → `SimulationReturnData([config(w), router(ro),
  reward(ro), operator_sol(index)(w)…])`, tudo derivado só da mensagem.
- `handle()` → credita `reward` lamports (do pool = config PDA) em cada
  `operator_sol(index)`.

### Endereços (produção)
- `pod` (programa): `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj`
  · 32B = `0x1a3be2685e7a787a1bedadcc90889b367f8fe72240de5aa43e4c2b88d07776a2`
- vault TC: `terra1gqkrh2…` · 32B =
  `0x402c3ba99da6c0d1fc257e45afe1574750604b9a4e3db6d6df6fc47ff4257579`
- Domínios: Solana `1399811149` · TC `132556`. Reward: `499000` lamports (taxa medida).

### Passo a passo (LOCAL — nada na VPS; as chaves são suas)
```bash
# 1) subir o pod atualizado (interface de recipient + WithdrawOperatorSol)
#    build: cargo build-sbf --manifest-path svm/programs/pod/Cargo.toml  → target/deploy/pod.so
solana program deploy svm/target/deploy/pod.so --program-id <pod_keypair>   # ou upgrade

# 2) migrate do vault do TC (idempotência SENT_RECEIPT) — preserva pool/registro
bash deploy/tc-remigrate.sh                       # wasm sha256 cb753ed7…563f19bd

# 3) config do corredor (dois lados)
node deploy/rrv-receipt-config-solana.mjs         # pod: router(TC)+operator_sol(+reward)
bash deploy/tc-receipt-config-solana.sh           # TC: router(Solana)+de/para

# 4) semear o pool do pod (config PDA) com algum SOL (paga as recompensas)
solana transfer Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w 0.05 --allow-unfunded-recipient
```

### Fluxo em produção (por operador, com o relayer nativo)
1. O operador entrega mensagens **Solana→TC** (relayer nativo, sem alteração).
2. No **TC**, chama `send_receipt` das próprias entregas (paga o gás; idempotente):
   ```bash
   terrad tx wasm execute $VAULT_TC \
     '{"send_receipt":{"messages":["<msg_hex>","<msg_hex2>"]}}' --amount <gas_igp>uluna $TX
   ```
3. O **relayer nativo** leva o recibo de volta ao `pod`, que credita o SOL na
   PDA `operator_sol(index)`.
4. O operador **saca** (rrv variante 6):
   ```
   WithdrawOperatorSol{index, amount}  — contas: [signer(carteira) w, opsol PDA(index) w]
   ```

### Escala para N operadores
Os 3 (ou N) rodam **só o relayer nativo**. Cada um chama `send_receipt` das suas
entregas e saca sua PDA. Não instalam nada. O índice do operador é o do **de/para
global** (o mesmo no TC e no `SetOperatorSol` da Solana).

> **Status: PROVADO EM PRODUÇÃO (2026-08-19).** Recibo Solana→TC entregue pelo
> relayer NATIVO no pod, `handle` pagou, operador sacou. Sem keeper.
>
> Provas (mainnet):
> - upgrade do pod: `24bTjQSAQpARHA3gKiiT8W7qRPLBMBPftabf3ppijXL6DSazNmVsD7Xsoi2GxRdD8hd7q3rpKZZa8TyGD739QF22`
> - migrate do vault TC → `code_id 11594` (wasm `cb753ed7…`), tx `9C503ED3F10F931A575ECA2A6048C8BD72EA600EBA023F8E82A2BB581BA4654D`
> - `send_receipt` (TC, 2 ids `d5e2ab02…`/`d039daa1…`): tx `FD720251DAA642AC7EE65C36BC7AFB977BD4C9729007D82204AA9AE23CBF67A3` (bloco 30021581) → recibo `5f67d0f7eec906e72bf724f1333b1657b6c924773ee88a6e33a62706a421158a`
> - recibo entregue na Solana: `ProcessedMessage` PDA `pFtaCoYr9UQaMLjVwD5SGp8KZeVDXnH8vqYxhDzmgZ6` existe → `handle` creditou 2×499000 = 998000 lamports em `opsol(0)` (`8pz9ToVy…`)
> - saque do operador: `7mf9HE9Ck5fYqRg2XnLt9VoArFw3HBYUjhsZmsY2GLh5yk79mnDNy8XDaqsCdvQ18NiXwQFT8XYXLEGcMqUecU5`
