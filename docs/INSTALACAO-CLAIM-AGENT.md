# Instalação do claim-agent na VPS — coleta automática de comissões

O `claim-agent` **emite os recibos sozinho** (a cada 5 min) em todas as chains vivas, e
as comissões caem nas suas **carteiras de operador**. Roda na VPS como serviço
`systemd`, assinando com uma **carteira-gatilho dedicada** (só paga o gás). Sua chave
real **nunca vai para a VPS**.

## Conceito: carteira-gatilho
- É uma carteira **nova, descartável**, com **só um pouco de gás** (BNB no BSC, LUNC no TC).
- Ela **assina** o `sendReceipt`/`send_receipt` e paga o gás.
- A **comissão SEMPRE cai na sua carteira de operador** (`terra1run…` / `0x8f08…`),
  porque o contrato paga o endereço do **registro de/para**, não quem assinou.
- Se a VPS ou a carteira-gatilho vazar, o atacante **só pega o troco de gás** — suas
  chaves reais e suas comissões ficam intactas.

## O que JÁ está instalado (feito)
Na VPS `31.97.91.4` (`~/claim-agent/`):
- `claim-agent-receipt.mjs` (o agente, sem `terrad` — usa cosmjs + ethers).
- `solana-epoch-reporter.mjs` (reporter do quórum TC→Solana — ver §final).
- `node_modules` → symlink para o do `oracle-agent`.
- `.claim-agent-seen.json` (estado local, pré-semeado).
- `/etc/systemd/system/claim-agent.service` (**enabled**, parado).
- `.env` (template, `chmod 600`).

## O que VOCÊ faz (2 passos)

### Passo 1 — criar e financiar as carteiras-gatilho
- **BSC:** crie uma carteira EVM **nova** (ex.: `cast wallet new`, ou MetaMask). Pegue
  a **chave privada hex `0x…`**. Envie **~0,05 BNB** para o endereço dela.
- **TC:** crie uma carteira Terra **nova**. Pegue o **mnemônico** (ou a chave hex de 32
  bytes). Envie **uns LUNC** (ex.: 200 LUNC) para o endereço dela.

> Dica: depois de pôr as chaves no `.env` (passo 2), o agente **imprime os endereços
> das carteiras-gatilho** no log ("mande BNB/LUNC p/ gás: …") — use para financiar.

### Passo 2 — pôr as chaves e iniciar
```bash
ssh root@31.97.91.4
nano ~/claim-agent/.env
```
Preencha (carteira-gatilho, **não** a real):
```
BSC_PRIVATE_KEY=0xSUACHAVEHEX_DA_GATILHO_BSC
TC_PRIVATE_KEY=chave_hex_32bytes_da_gatilho_TC
#   (alternativa p/ o TC:  TC_MNEMONIC=palavra1 palavra2 ... )
```
Inicie e acompanhe:
```bash
systemctl start claim-agent
tail -f ~/claim-agent/logs/agent.log      # ou: journalctl -u claim-agent -f
```
Você verá as rodadas a cada 5 min, os endereços das gatilhos e os `txhash` dos recibos
emitidos. As comissões chegam nas carteiras de operador em seguida (o relayer nativo
entrega o recibo de volta e a origem paga).

## Operação
```bash
systemctl status claim-agent      # estado
systemctl restart claim-agent     # após editar o .env
systemctl stop claim-agent        # pausar
tail -n 100 ~/claim-agent/logs/agent.log
```

## Testar sem chaves (DRY — só leitura)
```bash
cd ~/claim-agent && DRY=1 node claim-agent-receipt.mjs
```
Mostra o que ele emitiria (quantos pendentes por chain), sem assinar nada.

## Ajustes (env no `.env` ou no service)
- `--loop 300` → intervalo em segundos (padrão 5 min).
- `MIN_BATCH=3` → só emite quando juntar ≥ N entregas da mesma origem (amortiza gás).
- `DISPATCH_PAGES=100` → quantos dispatches recentes varrer.
- IGP do recibo: constantes `TC.igp` no script (10 LUNC por padrão, com folga).

## Segurança
- `.env` é `chmod 600` (só root lê).
- A carteira-gatilho **só tem gás**; recarregue quando esvaziar.
- A comissão nunca passa pela gatilho — cai direto na carteira de operador do de/para.
- O agente **não** entrega mensagens (isso é o relayer nativo) e **não** toca em
  contrato nativo — só dispara os recibos, que qualquer um poderia disparar.

## Corredores cobertos
- **TC→BSC** → `sendReceipt` no BSC (gás BNB) → comissão em **LUNC no TC**.
- **BSC→TC** → `send_receipt` no TC (gás LUNC) → comissão em **BNB no BSC**.
- **Solana→TC** → `send_receipt` no TC (gás LUNC) → comissão em **SOL na Solana**.
- **ETH** → automático quando o vault do ETH existir.

---

## Reporter do quórum (TC→Solana) — INSTALADO como serviço
O `~/claim-agent/solana-epoch-reporter.mjs` reporta as entregas **TC→Solana** (modelo
de quórum — ver `AUTOMACAO-CLAIMS.md`). Roda como serviço **`epoch-reporter.service`**
(systemd), carregando o `.env` do relayer (`EnvironmentFile=/root/hyperlane/.env`) e
assinando com a `SOLANA_PRIVATE_KEY` do relayer — que é o operador **PbEo** (registrado),
então o `SubmitEpochReport` é aceito. A cada hora ele reporta a época fechada nova;
época já reportada → "nada a fazer" (idempotente).
```bash
systemctl status epoch-reporter
tail -f /root/claim-agent/logs/reporter.log
# manual/DRY:
cd ~/claim-agent && node solana-epoch-reporter.mjs           # mostra o relatório sem enviar
```
**Ajuste pendente:** `reward_lamports` está em **1** (placeholder) — a recompensa por
entrega TC→Solana. Quando você definir o valor (governança do `pod`), os relatórios
futuros creditam esse valor; o operador saca os créditos com a instrução `Withdraw` do
`pod` (posso automatizar o saque também, se quiser).
