# Operadores e Quórum — como consultar, configurar e alterar

Guia operacional: quem está no quórum, como registrar os 2 operadores de hoje,
e como **adicionar/remover** conforme o conjunto crescer ou encolher.

## Conceitos em 30 segundos

- **Operador** = a carteira que o relayer usa NAQUELA chain (endereços diferentes
  por chain: `terra1…` no TC, `0x…` nas EVMs, pubkey base58 na Solana). Registre
  em cada chain o endereço daquela chain.
- **Quórum** = quantas submissões/aprovações de operadores DISTINTOS são
  necessárias para o contrato agir (aplicar preço no oracle; na Solana, também
  fechar época de créditos e executar propostas do vault).
- A lista vive **no estado do contrato** — só o **owner** (governança no TC,
  multisig nas remotas) muda operadores/quórum. Invariantes automáticos:
  `1 ≤ quórum ≤ nº de operadores` e a lista nunca fica vazia (a tx reverte).
- Regra dos 2 operadores atuais: com quórum **2-de-2**, um operador offline
  TRAVA a atualização de preço (a faixa continua protegendo e a emergência do
  owner continua disponível). Ao entrar o 3º operador, mude para **2-de-3**.

---

## Terra Classic (oracle-governor CosmWasm)

**Consultar quem está no quórum:**
```bash
terrad q wasm contract-state smart <GOVERNOR> '{"operators":{}}' --node $NODE
terrad q wasm contract-state smart <GOVERNOR> '{"config":{}}'    --node $NODE   # inclui quorum
```

**Registrar no deploy** (script já suporta):
```bash
OPERATOR2=terra1... QUORUM=2 bash deploy/tc-deploy.sh
```

**Adicionar / remover depois** (assina o OWNER — hoje sua carteira; depois, proposta de governança):
```bash
# adicionar o 3º e subir p/ 2-de-3:
terrad tx wasm execute <GOVERNOR> '{"set_operators":{"add":["terra1NOVO"],"remove":[]}}' $TXFLAGS
terrad tx wasm execute <GOVERNOR> '{"set_quorum":{"quorum":2}}' $TXFLAGS

# remover um (baixe o quórum ANTES se necessário — o contrato impede quórum > operadores):
terrad tx wasm execute <GOVERNOR> '{"set_quorum":{"quorum":1}}' $TXFLAGS
terrad tx wasm execute <GOVERNOR> '{"set_operators":{"add":[],"remove":["terra1SAIU"]}}' $TXFLAGS
```

> O **vault** do TC não tem lista de operadores de propósito: o `Claim` é aberto
> e paga só quem o Mailbox registra como executor da entrega.

---

## BSC / Ethereum (GasOracleGovernor.sol)

**Consultar** (a EVM guarda mapping, não lista — confira por endereço ou pelos
eventos `OperatorAdded/OperatorRemoved`):
```bash
cast call $GOVERNOR "isOperator(address)(bool)" 0xOPERADOR --rpc-url $RPC
cast call $GOVERNOR "operatorCount()(uint256)" --rpc-url $RPC
cast call $GOVERNOR "quorum()(uint256)"        --rpc-url $RPC
cast logs --rpc-url $RPC --address $GOVERNOR "OperatorAdded(address)"   # histórico
```

**Registrar no deploy:** `OPERATOR2=0x... QUORUM=2 PRIVATE_KEY=0x... bash deploy/evm-deploy.sh bsc`

**Adicionar / remover depois** (assina o OWNER do governor):
```bash
cast send $GOVERNOR "setOperators(address[],address[])" "[0xNOVO]" "[]" --private-key $PK --rpc-url $RPC
cast send $GOVERNOR "setQuorum(uint256)" 2 --private-key $PK --rpc-url $RPC
# remover: setOperators("[]","[0xSAIU]") — ajuste o quórum antes, se preciso
```

---

## Solana

São **duas listas independentes** (governor e vault):

### igp-oracle-governor — muda o MULTISIG (1 assinatura)
Instrução `SetOperators { add, remove }` (variante 3) e `SetQuorum` (variante 4).
Consultar: ler a config PDA (`["gov","-","config"]`) — os operadores estão no
struct `Config`. Ex.: `solana account <CONFIG_PDA> --output json` + decode borsh
(o `deploy/solana-init.mjs` imprime a PDA no deploy).

### relayer-reward-vault (rrv) — muda por PROPOSTA COM QUÓRUM (sem admin único)
Cada operador envia o MESMO envelope; executa ao atingir o quórum:
```
SubmitAdminAction { envelope: { nonce: N, action: AddOperator(<pubkey>) } }
SubmitAdminAction { envelope: { nonce: N, action: SetQuorum(2) } }
SubmitAdminAction { envelope: { nonce: N, action: RemoveOperator(<pubkey>) } }
```
- O `nonce` permite repetir a mesma ação no futuro sem colidir com proposta já executada;
- A PDA da proposta é derivada do hash do envelope — todos convergem na mesma
  conta sem combinar nada;
- Com o quórum atual 2-de-2, **os dois** precisam submeter para executar.

**Registrar no deploy:** `OPERATOR2=<pubkey> bash deploy/solana-deploy.sh`
(o init cria governor e rrv já com os 2 operadores e quórum 2).

---

## Checklist ao mudar o conjunto de operadores

1. Registrar o endereço certo **em cada chain** (4 listas: TC governor, BSC, ETH, SOL governor + SOL rrv);
2. Ajustar o quórum junto (recomendação: quórum = maioria simples, ex.: 2-de-3);
3. O novo operador configura o **oracle-agent** dele (chaves próprias!) e o relayer;
4. Nunca compartilhar chave entre operadores — colapsa o quórum em 1 entidade;
5. Registrar a mudança na proposta/anúncio público (auditabilidade).
