# Manual de Expansão — nova chain, novos operadores, novas associações

Como crescer o sistema sem quebrar nada. Três operações, nesta ordem de
frequência: **associar endereços (de/para)** · **adicionar operador** ·
**adicionar chain**. Endereços atuais: `REGISTRO-AUDITORIA.md`.

---

## 1. O que é a "associação (de/para)" — o vínculo de identidade

Um operador é UMA entidade com um endereço DIFERENTE em cada chain. A associação
(binding) registra isso **dentro do vault de cada chain de origem**, para que ele
saiba a quem pagar a taxa quando a entrega acontece em outra rede:

```
Vault do TC     : (terra1run9wz…, domínio 1399811149) → PbEo7Fn2…   "as entregas
Vault do TC     : (terra1run9wz…, domínio 56)         → 0x8f08…      NA chain X
Vault do TC     : (terra1run9wz…, domínio 1)          → 0xEF81…      feitas por
Vault da BSC    : (0x8f08…,       domínio 132556)     → terra1run9wz…  ESTE
Vault da ETH    : (0xEF81…,       domínio 132556)     → terra1run9wz…  endereço
Vault da Solana : (PbEo7Fn2…,     domínio 132556)     → terra1run9wz…  são minhas"
```

Leitura: "no vault de ORIGEM, o operador LOCAL declara qual é o SEU endereço
executor no domínio de ENTREGA". Só o owner/governança grava vínculos — é a
troca de confiança do modelo: o quórum atesta A ENTREGA, o vínculo prova A IDENTIDADE.

### Comandos por chain (owner)

```bash
# TC (vault v2)
terrad tx wasm execute $VAULT '{"set_remote_binding":{"operator":"<terra1_operador>","domain":<dom_entrega>,"remote_address":"<endereco_executor_lá>"}}' $TX
# remover: "remote_address": null

# BSC/ETH (vault v2)
cast send $VAULT "setRemoteBinding(address,uint32,string)" <operador_local> <dom_entrega> "<endereco_executor_lá>"
# remover: string vazia ""

# Solana (proposta administrativa com quórum — modelo em deploy/rrv-remote-config.mjs)
# AdminAction::SetRemoteBinding { domain, operator: <pubkey_local>, remote_address: "<endereco_lá>" }
```

---

## 2. Por id vs por época — os dois modos de pagamento da taxa de origem

| Modo | Onde | Como funciona | Por quê |
|---|---|---|---|
| **Por id** | TC, BSC, ETH | Cada `message_id` é atestado e pago individualmente (`AttestRemoteDelivery`/`attestRemoteDelivery`). Registro `remote_claimed[id]` garante 1 pagamento por mensagem, auditável id a id. | Nessas chains, guardar 1 registro por id custa centavos — vale a granularidade máxima. |
| **Por época** | Solana | As entregas são AGREGADAS por janela de 6 h e entram no `EpochReport.remote` (contagem × recompensa), sob o mesmo hash/quórum do relatório; saque via `Withdraw`. | Na Solana, 1 conta por id custaria ~0,0015 SOL de rent — MAIS que a taxa (0,0005). Agregar por época zera o custo extra. |

O resultado econômico é o mesmo (taxa real da origem → executor); muda só a
granularidade do registro. **Desde 19/08 este é o ÚNICO pagamento real** — as
recompensas de destino foram reduzidas a 1 unidade simbólica (fim do pagamento
duplo; ver CLAIM-REMOTO §3). A tabela vigente:

| Origem | Mecanismo | Valor por entrega |
|---|---|---|
| TC | por id (`AttestRemoteDelivery`) | 33 LUNC |
| BSC | por id (`attestRemoteDelivery`) | ≈1,81e12 wei (taxa real, recalibrável) |
| ETH | por id (`attestRemoteDelivery`) | ≈9,29e12 wei (taxa real) |
| Solana | por época (`EpochReport.remote`) | 499.000 lamports (taxa real) |

---

## 3. Adicionar um OPERADOR (novo relayer entra no sistema)

Checklist completo — papel a papel. Sempre que o conjunto passar de 1 operador
independente, **suba os quóruns para ≥ 2** (é o que transforma auto-atestação
em verificação cruzada).

### 3.1 O novo operador prepara
- 1 endereço por chain (TC/BSC/ETH/Solana) com saldo mínimo p/ gás;
- roda o próprio relayer Hyperlane e (recomendado) a própria instância do
  oracle-agent+claim-agent (`docs/ORACLE-AGENT.md`) com as SUAS chaves.

### 3.2 O owner/governança registra (por chain)
```bash
# TC — governor (preço) e vault (atestador remoto):
terrad tx wasm execute $GOV   '{"set_operators":{"add":["<terra1_novo>"],"remove":[]}}' $TX
terrad tx wasm execute $GOV   '{"set_quorum":{"quorum":2}}' $TX
terrad tx wasm execute $VAULT '{"set_remote_operators":{"attestors":["<atuais>","<terra1_novo>"],"quorum":2}}' $TX

# BSC/ETH — governor e vault v2:
cast send $GOV   "setOperators(address[],address[])" "[<0x_novo>]" "[]"
cast send $GOV   "setQuorum(uint256)" 2
cast send $VAULT "setRemoteOperators(address[],uint256)" "[<atuais>,<0x_novo>]" 2

# Solana — governor (multisig assina) e vault (proposta com quórum):
node deploy/register-solana-operator.mjs <pubkey_novo>     # governor: SetOperators
# governor: SetQuorum(2) — instrução gov variante 4
# vault: AdminAction::AddOperator(<pubkey_novo>) + AdminAction::SetQuorum(2)
```

### 3.3 Associações do novo operador
Grave os vínculos de/para dele em TODOS os vaults de origem (seção 1).

### 3.4 Conferência
`remote_config`/`remoteAttestorCount()`/config PDA devem listar o novo operador;
uma entrega de teste dele deve gerar os dois pagamentos (destino + origem).

---

## 4. Adicionar uma CHAIN (nova rede entra na ponte)

Pré-requisito: o warp/mailbox/IGP/ISM da nova rede já implantados (fora do
escopo deste sistema). Depois, 6 passos:

1. **Contratos deste sistema na nova rede** — conforme a VM:
   - EVM: `deploy/evm-deploy.sh` (governor+vault) + `deploy/evm-vault-v2.sh`
     (padrão v2) — copie um bloco `case` existente e ajuste endereços;
   - CosmWasm: padrão do `deploy/tc-deploy.sh`;
   - SVM: padrão do pod (`deploy/solana-deploy.sh`).
   Em todos: `IGP.beneficiary → vault` e `oracle.owner → governor`.
2. **Domínio novo nos governors existentes** — faixa de preço em cada chain que
   passa a cotar a nova rede: `set_bounds`/`setBounds`/`SetDomainConfig`
   (derive dos valores VIGENTES ÷3 ×3 — produção é a verdade, nunca a doc).
3. **Recompensas remotas** — em cada vault de origem que passa a ter mensagens
   entregues na nova rede: `set_remote_reward{<novo_dom>, <taxa média real>}`;
   e no vault DA nova rede: rewards para os domínios existentes.
4. **Associações** — vínculos de/para de cada operador com o endereço dele na
   nova rede (seção 1), nos dois sentidos.
5. **oracle-agent/claim-agent** — novo bloco em `config.json` (`chains.<nome>`):
   rpc, governor, oracle, chave em `.env`, e `claims{mailbox, vault, relayer,
   domain, rpc de getLogs}`. O modo âncora cria a referência sozinho na 1ª rodada.
6. **Auditoria** — criar `docs/AUDITORIA-<CHAIN>.md` (padrão dos existentes) e
   atualizar `REGISTRO-AUDITORIA.md`/`WARP-IGORFAKE.md`.

---

## 5. Regras de ouro (aprendidas em produção)

1. **Produção é a verdade** — faixa, preço e recompensa se derivam do valor
   vigente on-chain, nunca de documentação (ela envelhece).
2. **Quórum 1 é modo de teste** — com o owner = operador único. Dois ou mais
   operadores independentes ⇒ quórum ≥ 2 em TUDO (preço, época, atestação).
3. **Deploys são LOCAIS** — a VPS só roda os binários do relayer/validador e o
   agente; wasm/scripts de deploy nunca vão para lá.
4. **Recompensa remota ≈ taxa real** — mantém o pool neutro. Monitorar
   `total_remote_paid` vs arrecadação (Sweep/IGP) por chain.
5. **Relayer sincronizado é dinheiro** — relaying é permissionless; indexação
   atrasada = corrida perdida para concorrentes (aconteceu: `EaxLm3Hw…`).
