# Instalação do Operador — Relayer + Oracle + Vault (tc-proof-of-delivery)

> Guia consolidado para subir um **nó de operador** do Terra Classic Hyperlane em um VPS.
> **Quem opera o relayer roda o pacote inteiro:** relayer + validator + oracle-agent (preços de gás)
> + claim-agent/epoch-reporter (recibos e recompensas do vault). Este doc reflete o setup **real em
> produção**. Aprofundamentos: `RELAYER-VPS.md`, `ORACLE-AGENT.md`, `VAULT.md`,
> `INSTALACAO-CLAIM-AGENT.md`, `OPERADORES.md`, `INSTALACAO_E_EXECUCAO.md` (build/deploy dos contratos).

---

## 1. O que o operador roda (5 serviços systemd)

| Serviço | Papel | Comando (ExecStart) | Cadência |
|---|---|---|---|
| **hyperlane-validator** | assina checkpoints do Mailbox TC (S3) | `bin/validator --originChainName terraclassic --checkpointSyncer.type s3` | contínuo |
| **hyperlane-relayer** | **entrega** mensagens entre chains | `bin/relayer --db … --metrics :9091` | contínuo |
| **oracle-agent** | atualiza os **oráculos de gás** nas 4 redes (quórum + faixa) | `node src/index.js` | loop 4h (época 6h) |
| **claim-agent** | emite **recibos** e coleta **comissões** (carteira-gatilho) | `node claim-agent-receipt.mjs --loop 300` | 5 min |
| **epoch-reporter** | reporta **quórum de entregas TC→Solana** (recompensa) | `node solana-epoch-reporter.mjs --submit --loop 3600` | 1h |

> Extra (rede de segurança, opcional): `deliver-receipts` (timer) — plano B que entrega recibos BSC→TC
> presos >3min quando o relayer perde a sequence. O relayer é o **primário**.

O operador ganha pelo que **ENTREGA**: a comissão do IGP (pass-through) cai no relayer e a recompensa
sai do **vault** conforme a prova on-chain de entrega. Ver `TARIFAS-E-RECOMPENSAS.md`.

---

## 2. Pré-requisitos do VPS

- Ubuntu 22.04+ (produção: 4 vCPU / 8 GB / 80 GB SSD confortável). O relayer é o mais pesado.
- **Node.js v20+** (produção usa v22) — `oracle-agent`/`claim-agent`/`epoch-reporter`.
- **Rust 1.84 + build-sbf** só se for **compilar** os contratos (a maioria dos operadores usa os binários
  já publicados; ver `INSTALACAO_E_EXECUCAO.md §3`).
- Binários do Hyperlane (`relayer`, `validator`) — em `/root/hyperlane/bin/`.
- **Bucket S3** (para os checkpoints do validator) + credenciais AWS.
- **RPCs**: TC (LCD + RPC), BSC, Ethereum, Solana (Helius recomendado).

---

## 3. Layout de diretórios (padrão de produção)

```
/root/hyperlane/            # relayer + validator
  bin/{relayer,validator}
  runtime/config/mainnet_config.json   # chains, mailboxes, ISMs, IGP
  .env                                  # chaves + AWS/S3 + RUST_LOG
/root/oracle-agent/        # oracle-agent (preços de gás)
  src/index.js  src/chains/{terraclassic,evm,solana}.js
  .env                                  # TC/BSC/ETH/SOL_PRIVATE_KEY
  logs/agent.log
/root/claim-agent/         # recibos + recompensas
  claim-agent-receipt.mjs  solana-epoch-reporter.mjs  deliver-receipts-tc.mjs
  .env  rpc.env
  logs/{agent,reporter,deliver}.log
```

---

## 4. Carteiras (crie e ABASTEÇA antes de ligar)

| Carteira | Onde | Serve para | Abastecer com |
|---|---|---|---|
| **Relayer/comissão** `terra1run9wz…26mawp` | `.env` do hyperlane (`TERRA_PRIVATE_KEY`) | gás das entregas no TC + **dono do vault** + recebe comissão | LUNC |
| **Operador BSC** `0x8f085bAD…5291` | oracle-agent `BSC_PRIVATE_KEY` | submeter preço no StorageGasOracle da BSC | **BNB** (~0,03) |
| **Operador ETH** `0xEF818120…00ae` | oracle-agent `ETH_PRIVATE_KEY` | submeter preço no oracle da Ethereum | **ETH** (~0,01) |
| **Operador Solana** `PbEo7Fn…rrkS` | oracle-agent `SOL_PRIVATE_KEY` | submeter preço no governor Solana (quórum) | **SOL** (~0,1) |
| **Reserva Solana** `BirXd4Q…DEf1j` | keypair local | topup do operador Solana **+ upgrade authority do pod** ⚠️ | SOL |

> ⚠️ A reserva `BirXd4Q` é **também a upgrade authority** dos programas Solana — mantenha esse keypair
> **fora do VPS** (só na máquina de deploy). Não é preciso reserva para funcionar; o operador Solana
> gasta ~0,000005 SOL/época (só fee) — o rent do round agora é pago **uma vez** (fix de 2026-08).

---

## 5. Variáveis de ambiente

**`/root/hyperlane/.env`** (relayer + validator):
```
AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_REGION / S3_BUCKET   # checkpoints do validator
VALIDATOR_DB / RELAYER_DB                                            # dirs de cache
TERRA_PRIVATE_KEY / BSC_PRIVATE_KEY / ETH_PRIVATE_KEY / SOLANA_PRIVATE_KEY
RUST_LOG=info
```
**`/root/oracle-agent/.env`**: `TC_PRIVATE_KEY`, `BSC_PRIVATE_KEY`, `ETH_PRIVATE_KEY`, `SOL_PRIVATE_KEY`
(a `SOL_PRIVATE_KEY` é o **seed hex ed25519 de 32 bytes**, formato do relayer Hyperlane).
**`/root/claim-agent/.env`**: `BSC_PRIVATE_KEY`, `TC_PRIVATE_KEY`; **`rpc.env`**: os RPCs.

> `chmod 600` em todos os `.env`. Nunca versione.

---

## 6. Endereços de referência (mainnet)

| | Endereço | Domínio |
|---|---|---|
| Mailbox TC | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` | 132556 |
| Oracle Governor TC | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` | |
| IGP TC | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` | |
| **Vault (relayer reward)** | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` | |
| ValidatorAnnounce | `terra1gtnmdevekgxpvzej3wfy20e2n335gm3muwj6geduxxa86j3x70cq00asmy` | |
| Pod Solana (vault+governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` | 1399811149 |

**Validadores TC (ISM 3-de-4):** igorveras `71b2b8c3…`, tcv `1afd3d07…`, darksun `e6bb0401…`,
burnitall `5c374754…` (threshold **3**). Ver `ISM-VALIDADORES.md`.

---

## 7. Instalação passo a passo

```bash
# 1) binários hyperlane + config
mkdir -p /root/hyperlane/{bin,runtime/config}
#   copie bin/{relayer,validator} e runtime/config/mainnet_config.json
#   preencha /root/hyperlane/.env (§5) — chmod 600

# 2) oracle-agent
git clone <repo> /root/src && cp -r /root/src/oracle-agent /root/oracle-agent
cd /root/oracle-agent && npm ci
#   preencha .env (§5); ajuste governors/RPCs/domínios em src (TC=132556)

# 3) claim-agent (recibos + recompensas)
cp -r /root/src/deploy/* /root/claim-agent/   # scripts .mjs + node_modules
cd /root/claim-agent && npm ci
#   preencha .env + rpc.env

# 4) carteiras: ABASTEÇA (§4) — LUNC no relayer, BNB/ETH/SOL nos operadores

# 5) serviços systemd (units em §1) → habilitar
systemctl enable --now hyperlane-validator hyperlane-relayer oracle-agent claim-agent epoch-reporter
```

**Ordem importa** só na 1ª implantação dos CONTRATOS (ver `INSTALACAO_E_EXECUCAO.md §4` — governor →
posse do oracle em 2 passos → faixas por domínio → vault → `IGP.set_beneficiary=vault` → semear pool).
Para **operar** (contratos já no ar), a ordem dos serviços é livre.

---

## 8. Operação e monitoramento

- **Painel em tempo real:** `node deploy/monitor-web.mjs` → `http://localhost:8787` (carteiras, pools,
  serviços, validadores, RPCs). SSE com auto-refresh.
- **Logs:** `journalctl -u <svc> -f` (relayer/validator) · `tail -f /root/oracle-agent/logs/agent.log` ·
  `/root/claim-agent/logs/{agent,reporter}.log`.
- **Saúde mínima** (spec / skill `tc-pod-deploy`):
  - `LayoutCheck` no TC (pós-migrate do Mailbox) — evita parse errado.
  - **Solvência** do vault: `{"solvency":{}}` → `claims_payable` vs backlog.
  - Épocas Solana sem quórum (hashes divergentes = alarme).
  - Preço **não aplicado** por `DeltaExceeded` → avaliar `ForceSet` pela governança/multisig.

### Regras do oracle-agent (importante)
- Roda a cada **4h**; **época = 6h**. Só submete quando o **drift > 300 bps**; senão "estável".
- **Teto de delta = 2000 bps** por submissão: se o preço candidato variar >20% do último aplicado, é
  **rejeitado** (proteção). Fica travado até o mercado voltar OU um `ForceSet` da governança.
- **Solana:** desde 2026-08 o round é **uma conta por domínio** (rent pago 1×, reusado a cada época) —
  sem o antigo dreno de ~0,0151 SOL/época. Instrução `CloseRound` fecha rounds órfãs e devolve o rent.

---

## 9. Troubleshooting (casos reais)

| Sintoma | Causa | Ação |
|---|---|---|
| `insufficient lamports` (Solana) | operador sem SOL | transferir SOL p/ `PbEo7Fn…` (~0,1) |
| `gas_price delta too large … 2000 bps` | oscilação >20% | esperar mercado ou `ForceSet` (governança) |
| relayer `limit exceeded` (BSC RPC) | rate-limit do RPC público | benigno (retry) ou trocar o RPC |
| submissão EVM falha | operador BSC/ETH sem gás | transferir BNB/ETH p/ os endereços (§4) |
| reporter `429 Too Many Requests` | rate-limit Solana | benigno (retry) ou RPC dedicado (Helius) |

---

## 10. Segurança (dinheiro de terceiros)

- Chaves só no servidor, `.env` `chmod 600`, nunca versionar.
- **Upgrade authority** dos programas Solana e **owner** dos contratos → **multisig** (3 validadores TC
  + 1 não-validador; ISM 3-de-4). Enquanto a authority estiver num deployer, mantê-la **fora do VPS**.
- Efeito-antes-do-registro, parse estrito (`deny_unknown_fields`) e guards de replay são invariantes
  cobertos por 91 testes — **não** quebrar (ver skill `tc-pod-contratos`).
