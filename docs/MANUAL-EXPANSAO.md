# Manual de Expansão — nova chain, novos operadores, novas associações

Como crescer o sistema sem quebrar nada, no **modelo de recibo trustless**
(`RECIBO-TRUSTLESS.md`). Três operações: **registrar identidade (de/para)** ·
**adicionar operador** · **adicionar chain**. Endereços atuais:
`REGISTRO-AUDITORIA.md`.

---

## 1. O que é a "associação (de/para)" — o registro de identidade

Um operador é UMA identidade com um endereço DIFERENTE em cada chain. O registro
guarda isso por ÍNDICE — a mesma linha vale em todas as chains:

```
operador 0 = { TC: terra1run9wz…,  BSC: 0x8f08…,  ETH: 0xEF81…,  SOL: PbEo… }
operador 1 = { TC: terra1abc…,     BSC: 0x1234…,  ETH: 0x5678…,  SOL: 9xYz… }
```

Cada vault guarda esse registro (`operator_address[(índice, domínio)]`) e um
**reverse-lookup** para o domínio LOCAL (`endereço → índice`). O recibo carrega
só **(message_id, índice)**; cada chain paga o endereço do índice no SEU próprio
registro. É a troca de confiança do modelo trustless: os validadores provam A
ENTREGA (no recibo), o registro (só o owner grava) diz A IDENTIDADE/para onde pagar.

### Comandos (owner) — ver §3.2 para o passo a passo por chain

```bash
# TC
terrad tx wasm execute $VAULT '{"set_operator_address":{"index":<N>,"domain":<dom>,"address":"<endereco>"}}' $TX
# BSC/ETH
cast send $VAULT "setOperatorAddress(uint32,uint32,string)" <N> <dom> "<endereco>"
# remover: address null (TC) / "" (EVM) no domínio
```

---

## 2. Por id vs por época — os dois modos de pagamento da taxa de origem

O pagamento é sempre **na chain de ORIGEM** (quem cobrou a taxa), ao **operador
que ENTREGOU** (provado pelo `processor(id)` no destino + recibo assinado pelos
validadores). Muda só como o registro é guardado:

| Modo | Onde | Como funciona | Por quê |
|---|---|---|---|
| **Por id** | TC, BSC, ETH | Cada `message_id` é pago individualmente ao receber o recibo (`handle`). `remote_claimed[id]` garante 1 pagamento por mensagem, auditável id a id. | Guardar 1 registro por id custa centavos — vale a granularidade máxima. |
| **Por época** | Solana | Entregas AGREGADAS por janela de 6 h no `EpochReport.remote` (contagem × recompensa); saque via `Withdraw`. | Na Solana 1 conta por id custaria ~0,0015 SOL de rent — MAIS que a taxa. Agregar zera o custo. |

Tabela vigente (recompensa remota ≈ taxa real de origem):

| Origem | Mecanismo | Valor por entrega |
|---|---|---|
| TC | por id (recibo → `handle`) | 33 LUNC |
| BSC | por id (recibo → `handle`) | ≈2,26e12 wei (taxa real, recalibrável) |
| ETH | por id (recibo → `handle`) | ≈9,29e12 wei (taxa real) |
| Solana | por época (`EpochReport.remote`) | 499.000 lamports (taxa real) |

> **Modelo atual = RECIBO TRUSTLESS** (`RECIBO-TRUSTLESS.md`): o vault de destino
> prova a entrega e despacha um recibo assinado pelos validadores; o vault de
> origem paga ao receber. **Sem atestadores, sem quórum, sem agente com poder de
> decisão.** O modelo de atestação anterior (com quórum) está descrito em
> `CLAIM-REMOTO.md`/`SEGURANCA-CLAIMREMOTO.md` para referência histórica.

---

## 3. Adicionar um OPERADOR (modelo de recibo)

No modelo de recibo **não há quórum nem atestação** — cada operador é
INDEPENDENTE: quem entrega, recebe (o `processor(id)` prova quem foi, o registro
de/para diz para onde pagar). Adicionar operador = só **registrar os endereços
dele** (o owner grava; um dia o frontend faz isso numa tela).

### 3.1 O novo operador prepara
- 1 endereço por chain (TC/BSC/ETH/Solana) com saldo mínimo p/ gás;
- roda o próprio relayer Hyperlane (para entregar e ganhar as taxas de origem).

### 3.2 O owner registra o de/para (índice novo, ex.: 1) — em CADA vault

Regra: em cada vault, o endereço do **domínio LOCAL** alimenta o reverse-lookup
(é assim que o destino descobre "quem entregou aqui é o operador N"); os demais
domínios são o registro de para onde pagar na origem.

```bash
# --- Vault do TC (dom local 132556) ---
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":132556,"address":"terra1novo..."}}' $TX  # local
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":56,"address":"0xNOVO_BSC..."}}' $TX        # registro
terrad tx wasm execute $VAULT_TC '{"set_operator_address":{"index":1,"domain":1,"address":"0xNOVO_ETH..."}}' $TX

# --- Vault da BSC (dom local 56) ---
cast send $VAULT_BSC "setOperatorAddress(uint32,uint32,string)" 1 56     "0xNOVO_BSC..."     # local → reverse-lookup
cast send $VAULT_BSC "setOperatorAddress(uint32,uint32,string)" 1 132556 "terra1novo..."     # registro

# --- Vault da ETH (dom local 1) --- idem BSC, trocando 56→1
# --- Solana (pod) --- proposta administrativa (multisig):
#   AdminAction::SetRemoteBinding{ domain, operator: <pubkey>, remote_address } — modelo em deploy/rrv-remote-config.mjs
```

### 3.3 Conferência
```bash
# reverse-lookup: o executor local do operador 1 resolve para o índice 1?
cast call $VAULT_BSC "operatorOfLocal(address)(bool,uint32)" 0xNOVO_BSC...
terrad q wasm contract-state smart $VAULT_TC '{"operator_of_local":{"address":"terra1novo..."}}' --node $NODE
```
Uma entrega de teste do operador 1 → o recibo paga o endereço dele
(`remote_claimed[id].executor` = endereço de N na origem). Nenhum outro passo.

### 3.4 Remover um operador
`set_operator_address` com endereço vazio/`null` no domínio local remove o
reverse-lookup (ele deixa de ser reconhecido como executor); remova nos demais
domínios para limpar o registro.

---

## 4. Adicionar uma CHAIN (nova rede entra na ponte)

Pré-requisito: o warp/mailbox/IGP/ISM da nova rede já implantados (fora do
escopo deste sistema). Depois, 6 passos:

1. **Vault deste sistema na nova rede** (mesmo contrato, os 2 papéis):
   - EVM: `deploy/evm-vault-receipt.sh <chain>` (deploy + beneficiary + config);
   - CosmWasm: padrão do `deploy/tc-migrate-vault-receipt.sh`;
   - SVM: mesmo programa `pod` (sem deploy novo — só config).
   Em todos: `IGP.beneficiary → vault`.
2. **Router cruzado** — cada par de vaults registra o outro como router
   confiável: `set_remote_router{domain, <vault do outro>}` (o endereço é o
   canônico 32B / hex32 left-pad). É o que autoriza o `handle` e define o alvo do
   `send_receipt`. **Sem router mútuo, o recibo não é aceito nem despachado.**
3. **Recompensas** — em cada vault de origem, `set_remote_reward{<dom_destino>,
   <taxa real>}` para os destinos que ele passa a servir (produção é a verdade).
4. **de/para dos operadores** — registre o endereço de cada operador na nova
   rede em TODOS os vaults (§3.2), nos dois sentidos.
5. **Rota Hyperlane** — o vault precisa ser um recipient válido e o ISM de
   entrada aceitar a rota do recibo. Corredores que já têm warp bidirecional
   (o ISM já valida os 2 sentidos) usam o ISM default — sem config extra. Uma
   rota nova exige registrar o ISM/validadores dela (config de infra, sem tocar
   em contrato Hyperlane nativo).
6. **oracle-agent** — novo bloco em `config.json` (preço). O claim-agent de
   ATESTAÇÃO não é mais necessário no modelo de recibo; o operador (ou o
   frontend) chama `send_receipt` quando o `quote` compensa o gás.
7. **Auditoria** — `docs/AUDITORIA-<CHAIN>.md` + atualizar `REGISTRO-AUDITORIA.md`.

---

## 5. Regras de ouro (aprendidas em produção)

1. **Produção é a verdade** — faixa, preço e recompensa se derivam do valor
   vigente on-chain, nunca de documentação (ela envelhece).
2. **Modelo de recibo não tem quórum** — cada operador é independente; a prova é
   o `processor(id)` + o recibo assinado pelos validadores. Um operador só recebe
   por entregas que ELE fez, no endereço que o OWNER registrou.
3. **Router mútuo é obrigatório** — os dois vaults do corredor têm de registrar
   um ao outro (`set_remote_router`); é a allowlist que torna o `handle` seguro.
4. **Deploys são LOCAIS** — a VPS só roda os binários do relayer/validador e o
   oracle-agent; wasm/scripts de deploy nunca vão para lá.
5. **Recompensa remota ≈ taxa real** — mantém o pool neutro. Monitorar
   `total_remote_paid` vs arrecadação (Sweep/IGP) por chain.
6. **Relayer sincronizado é dinheiro** — relaying é permissionless; indexação
   atrasada = corrida perdida para concorrentes (aconteceu: `EaxLm3Hw…`).
7. **O operador decide quando sacar** — sem limiar on-chain; consulte `quote`
   e agrupe entregas (batching) até o recibo compensar o gás.
