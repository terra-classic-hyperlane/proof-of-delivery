# Automação de Comissões — claim-agent + epoch-reporter

> Dois agentes off-chain que emitem/reportam as comissões automaticamente em todas as
> chains. **Nenhum contrato é alterado; nenhum relayer é customizado** — os agentes só
> observam a chain e disparam as transações que qualquer operador já faria à mão.

Resumo:
| Corredor | Como a comissão é reivindicada | Ferramenta |
|---|---|---|
| TC→BSC, BSC→TC, Solana→TC | **modelo de recibo** (emite recibo no destino) | `claim-agent-receipt.mjs` |
| **TC→Solana** | **quórum de operadores** (relatório de época) | `solana-epoch-reporter.mjs` |

Por que dois: no modelo de recibo o destino **prova a entrega on-chain** e o ISM
valida. Isso funciona onde o destino grava o executor (TC/BSC/ETH). A Solana **não
grava o executor**, então o sentido TC→Solana usa **quórum**: os operadores observam
off-chain quem entregou e submetem o mesmo relatório; a maioria honesta credita.

---

## 1. `claim-agent-receipt.mjs` — modelo de recibo (TC↔BSC + Solana→TC)

Para cada chain de DESTINO, acha as entregas feitas **pelo operador** que ainda não
foram pagas, batela por origem e emite o recibo:
- **TC** (`send_receipt`) → paga **BSC→TC** (BNB) e **Solana→TC** (SOL)
- **BSC** (`sendReceipt`) → paga **TC→BSC** (LUNC no TC)
- **ETH** — quando o vault do ETH existir (auto-skip)

A comissão sempre cai na chain de **origem**; o agente só dispara. O relayer nativo
entrega o recibo de volta e a origem paga sozinha.

**Rodar:**
```bash
# ver o que faria (sem chaves, só leitura):
DRY=1 node deploy/claim-agent-receipt.mjs
# emitir 1 rodada (precisa das chaves):
BSC_PRIVATE_KEY=0x… TC_KEYRING_PASS='senha' node deploy/claim-agent-receipt.mjs
# serviço (a cada 5 min):
BSC_PRIVATE_KEY=0x… TC_KEYRING_PASS='senha' node deploy/claim-agent-receipt.mjs --loop 300
```

Detalhes:
- **Descoberta sem `getLogs`** (o RPC público do BSC não suporta): varre os dispatches
  do TC (`tx_search`) e confirma estado com `eth_call`/query.
- **Dedup por origem** (evita reemitir e gastar gás): TC→BSC checa `remote_claimed` no
  TC; BSC→TC checa `remoteClaimed` no BSC; Solana→TC usa estado local
  (`deploy/.claim-agent-seen.json`) + a idempotência on-chain do `send_receipt`.
- **Exclui recibos** (recipient == vault) para não "receber comissão de recibo".
- **Batching**: junta N entregas da mesma origem num recibo só (1 gás).

> Economia: cada recibo custa gás; junte várias entregas. O imposto do Terra (~1,5%)
> incide 1× por transferência de saída, não por id — mais um motivo pra batelar.

---

## 2. `solana-epoch-reporter.mjs` — quórum (TC→Solana)

O relayer NATIVO entrega as msgs TC→Solana (nada muda). O reporter **observa off-chain
quem entregou** (fee payer da tx de entrega, lido da `ProcessedMessage`), monta o
`EpochReport` e submete ao `pod`. Quando um **quórum** de operadores submete o MESMO
relatório (hash idêntico), o contrato credita cada operador; cada um **saca** do pool.

Determinístico p/ o quórum: cada entrega é atribuída a uma época pelo `blockTime` do
seu slot (tudo lido da chain), então todos os operadores chegam ao MESMO relatório.

**Confiança:** maioria honesta do quórum — **o mesmo que você já deposita nos
validadores** (e onde operador = validador, é o mesmo grupo). Não é a prova
criptográfica do ISM (impossível aqui, a Solana não grava o executor), mas é
descentralizado e sem agente único.

**Rodar:**
```bash
# ver o relatório da última época fechada (só leitura):
node deploy/solana-epoch-reporter.mjs
# de uma época específica:
node deploy/solana-epoch-reporter.mjs --epoch 82736
# submeter (assina como operador do rrv; cada operador do quórum roda isso):
node deploy/solana-epoch-reporter.mjs --submit
# saque depois: o operador saca sua PDA de crédito (Withdraw do pod)
```

Detalhes:
- **Só credita operadores registrados** (`config.operators`) por padrão — o pool não
  paga relayers estranhos. `INCLUDE_ALL=1` credita qualquer um que entregou (modo
  permissionless).
- **Ativação:** quórum ≥ maioria (com 2 operadores, `quorum=2` p/ ser trustless de
  fato; com `quorum=1` é um operador só) e `reward_lamports` > 0 (**hoje está em 1,
  placeholder — ajuste**). Config via ação administrativa do `pod` (governança).

> **PROVADO EM PRODUÇÃO:** o operador `PbEo7Fn2…` foi **creditado 6.000.000 lamports e
> sacou os 6.000.000** via este mecanismo — o ciclo TC→Solana (entrega nativa →
> relatório de quórum → crédito → saque) funciona fim a fim.

---

## Regras que ambos respeitam
- Relayer nativo do Hyperlane **sem alteração** (os agentes não entregam mensagens).
- **Nenhum contrato nativo** do Hyperlane tocado.
- **Sem keeper** (relayer customizado). O reporter é um observador, não um relayer.
- Escala para **N operadores**: cada um roda o(s) agente(s); no quórum, a maioria
  honesta credita.

Endereços e conversões: `GUIA-OPERADORES-VALIDADORES.md`. Modelo de recibo:
`RECIBO-TRUSTLESS.md`. Auditoria de pagamentos: `AUDITORIA-COMISSOES.md`.
