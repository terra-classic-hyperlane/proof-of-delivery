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

## Teste de ponta a ponta (18/08 20:49–20:53 UTC — submissões REAIS)

Com `minChangeBps=0` temporário, uma rodada real submeteu nas 6 rotas:

| Rota | Tx | Resultado |
|---|---|---|
| TC → dom 1 (ETH) | `CA15CE3C0ED2B29D1F3028E3ABB9EEF214D2BD25C559E463D8A9B800EC0CBA92` | ✅ aplicado |
| TC → dom 56 (BSC) | `044EE80E81B049D95C08E67B1508E63E46220810FC36631EB586292E7C627D28` | ✅ aplicado |
| TC → dom SOL | `1DCA67D5B6C2DDB383BEF9EECA6F02383B3D0C7140356D4E524901D16B90D8D2` | ✅ aplicado |
| BSC → dom 132556 | `0x87cb7dc1af9066c35d710c52a8d7d866dc4bcd0eb9eef4e32467d2db663e3300` | ✅ aplicado |
| ETH → dom 132556 | `0x453dc7213306e940cb63f0e10111cb70a1009d6c960e48f144bfe1285bce5ce3` | ✅ aplicado |
| SOL → dom 132556 | `wbYQMDyobCgkoReWph9SPQMcqUoLTxYAMhJqoWovxs5vkMtRV7LhJh37o4AUg5PRZjhLwqiSqu7n2AtkBm12Ttk` | ✅ aplicado (IGP: 29400000000 → 29484263762, verificado) |

Dois defeitos encontrados e corrigidos pelo teste:
1. **config sem `privateKeyEnv` na Solana** → 1ª tentativa falhou com `env
   SOLANA_KEYPAIR_PATH ausente`; corrigido no config.json e no example.
2. **Quórum 2 no governor Solana** (o init rodou com OPERATOR2 no ambiente) →
   submissão era gravada mas nunca aplicava (CPI ausente nos logs da tx
   `eEH96Mtq…`). Ajustado p/ quórum 1 via multisig (tx `bbpnAfwZ…`); reteste
   aplicou no IGP real. `minChangeBps` restaurado p/ 300; serviço ativo.

## Adendo (18/08 22:27 UTC): claim-agent integrado e ativo

- `src/claims.js` adicionado ao MESMO serviço (fase 2 de cada rodada de 1 h):
  claim automático TC/EVM + relatório de época/withdraw na Solana.
- Scanner TC validado contra a entrega REAL `d039daa1…a28c4f04` (bloco
  29422362, relayer terra1run9wz…): id e sender extraídos corretamente.
  Essa entrega específica está EXPIRADA para claim (586.753 blocos > janela de
  200.000) — o resgate automático vale para entregas novas.
- **Quórum do vault Solana reduzido 2→1** pelo fluxo de proposta §09 com as DUAS
  aprovações (BirXd4 `VRQUgUzx…`, PbEo `f2DPjZdB…` — executa na 2ª): sem isso,
  relatórios de época jamais creditariam com 1 operador ativo. De quebra, o
  fluxo de admin multi-operador foi validado em produção.
- Cursors iniciais gravados nas 4 chains (TC 30009127 · BSC 116736155 ·
  ETH 25784960 · SOL 3LRV8CuM…). Serviço `active`, próximas rodadas a cada 1 h.
