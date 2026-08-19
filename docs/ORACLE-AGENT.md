# oracle-agent + claim-agent — instalação, execução e auditoria

O agente off-chain do operador: a cada **1 hora** (`intervalSeconds: 3600`) cota
preços (CoinGecko) e gás, e submete `SubmitPrice` aos governors das 4 redes.
Quórum, mediana, faixa e delta são aplicados **on-chain** — o agente não decide nada.

## Fase 2 de cada rodada: CLAIMS (o claim-agent)

No mesmo processo/serviço, após os preços, o agente **resgata os pagamentos
das entregas do relayer** (`src/claims.js`, config `chains.<nome>.claims`):

- **TC**: varre `wasm-mailbox_process_id` no Mailbox (tx_search) filtrando
  `message.sender = relayer`, confere `claimed`/solvência e chama `Claim` em lote.
- **BSC/ETH**: varre o evento `ProcessId` do Mailbox (getLogs, janelas de 2000
  blocos), filtra `mailbox.processor(id) = relayer`, confere `claimedBy`/pool e
  chama `claim(ids)`. Sem pool, os ids ficam PENDENTES no state (semear o vault).
- **Solana**: conta os `process()` pagos pelo relayer no Mailbox por época;
  época fechada → `SubmitEpochReport` (módulo rrv do pod; quórum on-chain) →
  `Withdraw` do crédito disponível (respeitando o rent do pool).
- **v2 ClaimRemote (taxa de ORIGEM)**: o scanner do TC captura a ORIGEM de cada
  entrega sua e o agente atesta no vault de origem — TC atesta por id
  (`remoteAttest`), BSC/ETH por id (`remoteAttestEvm`), Solana por época
  (`remoteAttestSol` → campo `remote` do relatório). Tudo no MESMO ciclo horário.

Cursors/pendências/épocas ficam no `state.json`. **Primeira rodada só grava o
cursor** — apenas entregas NOVAS são resgatadas automaticamente (antigas:
manual, `OPERACAO-CONTRATOS.md`). Janela TC: 200k blocos; BSC 1,6M; ETH 100,8k.

## Modo ÂNCORA (por que ele não calcula preço do zero)

Cada deployment do warp tem **calibração própria** (ex.: BSC vigente 9047190;
a fórmula canônica daria 789). Por isso o agente:

1. na 1ª rodada, **lê o valor VIGENTE on-chain** de cada oracle e grava como
   âncora em `state.json` (junto com o ratio de preço USD do momento) —
   **nada é submetido**;
2. nas rodadas seguintes, calcula o candidato = âncora × (variação relativa do
   preço USD) e gás = âncora × (variação do gás observado);
3. só submete se o drift vs o vigente ≥ `minChangeBps` (default 300 = 3%).

Recalibraram o oracle manualmente? **Apague a entrada correspondente do
`state.json`** e o agente re-ancora no novo valor na rodada seguinte.

## Chaves — TUDO em HEX (as mesmas do relayer Hyperlane)

| Env (`.env`) | Chain | Formato |
|---|---|---|
| `TC_PRIVATE_KEY` | Terra Classic | secp256k1 hex (cosmosKey) |
| `BSC_PRIVATE_KEY` / `ETH_PRIVATE_KEY` | BSC / Ethereum | secp256k1 hex |
| `SOL_PRIVATE_KEY` | Solana | seed ed25519 hex (32 bytes) |

A conta de cada chave precisa estar **registrada como operador** no governor da
respectiva chain (ver `OPERACAO-CONTRATOS.md`) e ter saldo mínimo p/ gás.

## Instalação em produção (VPS do relayer) — estado 18/08/2026

Já instalado em `/root/oracle-agent` (Node v22.14.0, deps `--omit=dev`):

```bash
# 1. .env com as chaves do relayer (RODAR UMA VEZ — chaves ficam no servidor):
ssh root@31.97.91.4 'bash /root/oracle-agent/setup-env.sh'
# 2. validar sem assinar nada:
ssh root@31.97.91.4 'cd /root/oracle-agent && node src/index.js --once --dry-run'
# 3. ativar o serviço (loop de 1h, reinício automático):
ssh root@31.97.91.4 'systemctl enable --now oracle-agent && systemctl status oracle-agent --no-pager'
```

Config de produção: `/root/oracle-agent/config.json` (4 chains habilitadas,
governors/oracles reais, intervalo 3600 s). Unit: `/etc/systemd/system/oracle-agent.service`.

## Logs e auditoria

- **Log contínuo (auditável):** `/root/oracle-agent/logs/agent.log` — cada linha
  tem timestamp ISO, chain, domínio, vigente, candidato, drift e o **hash da tx**
  de cada submissão. Também no journal: `journalctl -u oracle-agent`.
- **Âncoras:** `/root/oracle-agent/state.json` (valor + ratio + timestamp de
  cada âncora — evidência de qual calibração o agente está preservando).
- **Trilha on-chain (independente do log):** queries `submissions`/`applied`
  do governor TC, eventos `PriceSubmitted/PriceApplied` nos governors EVM,
  e as txs de `SubmitPrice` da wallet operadora em cada explorer.

```bash
ssh root@31.97.91.4 'tail -50 /root/oracle-agent/logs/agent.log'   # últimas rodadas
ssh root@31.97.91.4 'cat /root/oracle-agent/state.json'            # âncoras vigentes
```

## Comandos úteis

```bash
node src/index.js --once --dry-run   # simula uma rodada, não assina nada
node src/index.js --once             # uma rodada real (cron manual)
npm test                             # testes unitários
systemctl restart oracle-agent       # após editar config.json
```

## Solução de problemas

| Sintoma no log | Causa provável | Ação |
|---|---|---|
| `âncora criada … nada submetido` | 1ª rodada daquele domínio | Normal |
| `estável (<300bps)` | preço não mexeu o suficiente | Normal |
| `NotOperator` / `Unauthorized` | chave não registrada como operador | `OPERACAO-CONTRATOS.md` (SetOperators) |
| `BoundsExceeded` | candidato fora da faixa do governor | Investigar preço OU ajustar faixa (owner) |
| `DeltaExceeded` | salto > 20% numa época | Esperado em crash/pump; avaliar ForceSet |
| `env … ausente` | `.env` não criado/carregado | rodar `setup-env.sh`, checar unit |
