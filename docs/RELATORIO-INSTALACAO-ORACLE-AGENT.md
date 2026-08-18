# Relatório de Instalação — oracle-agent (auditoria)

**Data:** 2026-08-18 · **Servidor:** VPS do relayer Hyperlane (31.97.91.4, Ubuntu 24.04.4)
**Instalado por:** Claude Code, a pedido do operador (igor.veras@gmail.com)

## O que foi instalado

| Item | Valor |
|---|---|
| Runtime | Node v22.14.0 (tarball oficial → /usr/local) · npm 10.9.2 |
| Código | `/root/oracle-agent` (rsync deste repo, commit da instalação) · deps `npm install --omit=dev` |
| Config | `/root/oracle-agent/config.json` — 4 chains habilitadas · **intervalo 3600 s (1 h)** · `minChangeBps` 300 |
| Serviço | `/etc/systemd/system/oracle-agent.service` (Restart=always, RestartSec=60) |
| Logs | `/root/oracle-agent/logs/agent.log` (append; + journald) |
| Chaves | `.env` criado por `setup-env.sh` **executado pelo operador** (hex do relayer; nunca saíram do servidor; chmod 600) |

## Mudanças de código feitas para esta instalação (commit desta data)

1. **Chave HEX universal** — os 3 módulos aceitam a chave hex do relayer:
   TC `DirectSecp256k1Wallet.fromKey` · EVM `Wallet(hex)` · Solana `Keypair.fromSeed(hex)`.
2. **Modo ÂNCORA** — o agente lê o valor VIGENTE de cada oracle e só o ajusta
   pela variação relativa (não calcula do zero). Motivo: o dry-run pré-instalação
   revelou que a fórmula canônica divergia da calibração de produção (ex.: BSC
   calcularia 789 vs 9047190 vigente — toda submissão seria rejeitada pelas
   faixas ou, com faixas largas, quebraria as tarifas do warp).
3. `readOracle()` por chain (query CosmWasm / call EVM / scan do account Igp).

## Validação (dry-run NO SERVIDOR, 18/08 20:07 UTC — sem assinar nada)

```
[agent] oracle-agent iniciando · chains: terraclassic, bsc, ethereum, solana · DRY-RUN
[agent] preços USD: {"terraclassic":0.00004749,"ethereum":1911.19,"bsc":602.37,"solana":77.01}
[ethereum]     domínio 132556: âncora seria criada no vigente rate=26585078     gas=10000000000
[terraclassic] domínio 1:      âncora seria criada no vigente rate=376          gas=10000000000
[solana]       domínio 132556: âncora seria criada no vigente rate=29400000000  gas=28325
[terraclassic] domínio 56:     âncora seria criada no vigente rate=1098         gas=3000000000
[terraclassic] domínio 1399811149: âncora seria criada no vigente rate=383001553014 gas=1
[bsc]          domínio 132556: âncora seria criada no vigente rate=9047190      gas=10000000000
```

✅ As 6 rotas leram EXATAMENTE os valores de produção documentados em
`WARP-IGORFAKE.md` — leitura on-chain validada nas 4 redes antes de ativar.

## Operadores registrados nos governors (estado na instalação)

| Chain | Operador (= chave do `.env`) | Registrado? |
|---|---|---|
| Terra Classic | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` | ✅ (deploy Fase 1-2) |
| BSC | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` | ✅ (deploy Fase 3) |
| Ethereum | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` | ✅ (deploy Fase 3) |
| Solana | `PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS` | ✅ tx `284meW1GYxVrjc4KGbrFU5kBccL4hVpda1DVwZRmLZLHZ1xmuDL2NWGxrdyLvAt25AhbL7ExC2LSmpzcNhfQ3xdW` (verificado no gov config) |

## Ativação (18/08 20:12 UTC, autorizada explicitamente pelo operador)

`.env` criado (chmod 600) · operador Solana registrado · `systemctl enable --now
oracle-agent` → serviço **active**. Primeira rodada REAL (log de produção):

```
[agent] oracle-agent iniciando · chains: terraclassic, bsc, ethereum, solana
[ethereum]     132556: âncora criada no vigente rate=26585078     — nada submetido
[bsc]          132556: âncora criada no vigente rate=9047190      — nada submetido
[terraclassic] 1:      âncora criada no vigente rate=376          — nada submetido
[terraclassic] 56:     âncora criada no vigente rate=1098         — nada submetido
[terraclassic] 1399811149: âncora criada no vigente rate=383001553014 — nada submetido
[solana]       132556: âncora criada no vigente rate=29400000000  — nada submetido
[agent] próxima rodada em 3600s (loop)
```

✅ As 6 âncoras criadas exatamente nos valores de produção; nenhuma submissão na
estreia (comportamento projetado). A partir da próxima rodada (1 h), submete
apenas se o drift ≥ 3%, sempre dentro de faixa/delta/quórum on-chain.
