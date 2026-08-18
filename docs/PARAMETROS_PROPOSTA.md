# Parâmetros de partida — proposta baseada em custos reais

Valores sugeridos para a proposta de governança, ancorados em medições de
18/08/2026. **Tudo aqui é ponto de partida ajustável** — a governança (TC) e o
multisig (remotas) podem recalibrar depois; o delta/faixa do oracle e a tarifa
do vault são atualizáveis sem redeploy.

## 0. Evidências usadas (18/08/2026)

| Medição | Valor | Fonte |
|---|---|---|
| `process()` real no TC | **gas_used 508.260 · gas_wanted 655.344** | tx `4126C514…` bloco 29422362 (mainnet) |
| Preço mínimo de gás TC | **28,325 uluna/gas** (outra tx do mesmo bloco: 28,5) | RPC tx_search |
| Gas price BSC | 0,05 gwei | eth_gasPrice publicnode |
| Gas price Ethereum | ~0,22 gwei (histórico 0,2–5) | eth_gasPrice publicnode |
| Rent da PDA `ProcessedMessage` (SOL) | (128+56)×3480×2 ≈ **1.280.640 lamports** ≈ 0,00128 SOL | fórmula de rent + size 56 |
| Preços USD | LUNC 0,00004739 · BNB 602,81 · ETH 1.912,92 · SOL 76,84 | CoinGecko (dry-run do oracle-agent) |

> ⚠️ **BUG ENCONTRADO NO RELAYER ATUAL:** a tx real de `process()` pagou
> **28.325 uluna/gas** (mil vezes o mínimo de 28,325) — fee de 18.562 LUNC
> (~US$ 0,88) numa entrega que custaria ~18,6 LUNC (~US$ 0,0009). Quase
> certamente `gasPrice: "28325uluna"` em vez de `"28.325uluna"` na config do
> relayer. **Corrigir antes de qualquer cálculo de sustentabilidade.**

## 1. Custo real por entrega (com o gás correto)

| Rede | Cálculo | Custo | ≈ USD |
|---|---|---|---|
| Terra Classic | 655.344 gas × 28,325 uluna | ~18,6 LUNC | $0,0009 |
| BSC | ~300.000 gas × 0,05 gwei | 0,000015 BNB | $0,009 |
| Ethereum | ~300.000 gas × 0,22–1 gwei | 0,000067–0,0003 ETH | $0,13–0,57 |
| Solana | rent 1.280.640 + fee ~25.000 lamports | ~0,0013 SOL | $0,10 |

## 2. Tarifa por entrega (`reward_per_delivery`) — ~2–3× o custo

| Rede | Proposta | Em unidades mínimas | ≈ USD | Margem |
|---|---|---|---|---|
| Terra Classic | **50 LUNC** | `50000000` uluna | $0,0024 | 2,7× |
| BSC | **0,00005 BNB** | `50000000000000` wei | $0,030 | 3,3× |
| Ethereum | **0,0004 ETH** | `400000000000000` wei | $0,77 | cobre até ~1,3 gwei |
| Solana | **0,003 SOL** | `3000000` lamports | $0,23 | 2,3× |

Invariante de solvência: tarifa < arrecadação média do IGP por mensagem.
Monitorar com `Solvency`/`claimsPayable` vs backlog; ajustar por
governança/quórum se o pool drenar.

## 3. Janela de resgate (`claim_window_blocks`) — alvo ~14 dias

| Rede | Bloco médio | Proposta |
|---|---|---|
| Terra Classic | ~6 s | **200.000** |
| BSC | ~0,75 s (pós-Maxwell — **confirmar**) | **1.600.000** |
| Ethereum | 12 s | **100.800** |
| Solana | n/a — créditos por época não expiram | — |

## 4. Oracle — época, delta e faixas por domínio

- `epoch_duration_secs` = **21.600** (6h) · `max_delta_bps` = **2.000** (20%)
- Faixas: **min = vigente ÷ 3 · max = vigente × 3** — e o "vigente" é LIDO DO
  ORACLE EM PRODUÇÃO pelo próprio script NO MOMENTO do deploy (nada de valor
  fixo: doc envelhece, a chain não). As tabelas abaixo são apenas o RETRATO de
  18/08/2026 para referência/auditoria. Delta de 20%/época limita a deriva.

### No Terra Classic — CONVENÇÃO REAL do cw-hyperlane (medida on-chain 18/08)

Fórmula da IDA (guia oficial do deploy TC): `fee_uluna = gas × gas_price_destino ×
exchange_rate / 1e12`, com `exchange_rate = (LUNC_USD / NATIVO_USD) × 1e12` —
é a razão **local/remoto** (inversa da canônica!). Faixas = vigentes ÷3/×3:

| Domínio | vigente (rate · gas) | faixa rate | faixa gas_price |
|---|---|---|---|
| 1 (Ethereum) | 376 · 1e10 (10 gwei) | [125 · 1.128] | [3,33e9 · 3e10] wei |
| 56 (BSC) | 1.098 · 3e9 (3 gwei) | [366 · 3.294] | [1e9 · 9e9] wei |
| 1399811149 (Solana) | 383.001.553.014 · 1 (modelo lamport) | [1,28e11 · 1,15e12] | [1 · 10] |

### Nas remotas (domínio 132556 = Terra Classic) — CONVENÇÃO REAL (medida on-chain 18/08)

⚠️ Os IGPs remotos são CUSTOM (`TerraClassicIGPStandalone` + `TerraClassicOracle`
no EVM; overhead-IGP na Solana) com calibração própria validada em produção
(`tc-cw-hyperlane/terraclassic/doc/WARP-GAS-CONFIG.md`). As faixas ancoram os
VALORES VIGENTES (÷3 · ×3) — não a convenção teórica:

| Chain local | valores vigentes (rate · gas) | faixa rate | faixa gas_price |
|---|---|---|---|
| BSC | 9.047.190 · 1e10 | [3.015.730 · 27.141.570] | [3,33e9 · 3e10] |
| Ethereum | 26.585.078 · 1e10 | [8.861.692 · 79.755.234] | [3,33e9 · 3e10] |
| Solana (scale 1e19 + 10^(9−decimals)) | 2,94e10 · 28.325 · decimals 6 · overhead 3e6 | [9,8e9 · 8,82e10] | [9.442 · 84.975] |

Fórmulas validadas: EVM `wei=(gas+overhead)×gasPrice×rate/1e10` · Solana
`lamports=(gas+overhead)×gasPrice×rate/1e19×10^(9−decimals)`.
Oracle EVM: `setRemoteGasData(uint32,uint128,uint128)` FLAT (selector 0x666af432)
— o GasOracleGovernor.sol usa esta assinatura. TODO: recalibrar a fórmula EVM/SOL
do oracle-agent para a convenção por alvo (hoje ele calcula na convenção canônica).

## 5. Operadores, quórum e época de entregas (Solana)

- Partida: **2 operadores, quórum 2-de-2** (os dois agentes atuais) —
  funcional, mas sem tolerância a queda; **meta imediata: 3 operadores,
  quórum 2-de-3** (spec §12: sistema aberto, o 3º entra sem pedir licença).
- Épocas de entrega (Solana): 6h + folga de finalidade de **32 slots (~13s)**
  antes de fechar o relatório; janela de slots no relatório = a época em slots.

## 6. Multisig e ISM (remotas) — MODELO APROVADO PELA GOVERNANÇA

- **Composição aprovada**: multisig dos validadores Hyperlane — **3 validadores
  que validam o TC + 1 signatário que NÃO valida** (4 membros). É essa conta
  multiassinatura que recebe o owner do Vault/Governor/IGP/ISM nas remotas ao
  fim da implantação (até lá, owner = deployer).
- **Sobre o threshold** (decisão em aberto dentro do modelo aprovado):
  - `3-de-4`: tolera 1 ausente, MAS os 3 validadores sozinhos atingem o
    threshold — o alerta da spec §12 (quem valida checkpoints controlando o ISM
    tem acesso indireto ao colateral) fica mitigado só parcialmente;
  - `4-de-4`: exige o não-validador sempre (neutraliza conluio), porém sem
    tolerância a falha — 1 chave perdida trava tudo;
  - Evolução recomendada: adicionar um **2º não-validador** (3 val + 2 ext,
    threshold **4-de-5**) — validadores sozinhos não agem E tolera 1 ausente.
- ISM **3-de-4** com os 4 validators (tolera 1 offline; forjar exige 3).
- Timelock de **48h** para troca de ISM (executável no texto da proposta).

## 7. Semente dos pools

Pool inicial = **100× a tarifa** por rede (cobre o primeiro ciclo antes de o
Sweep/claim do IGP alimentar): TC 5.000 LUNC (~$0,24) · BSC 0,005 BNB (~$3) ·
ETH 0,04 ETH (~$77) · SOL 0,3 SOL (~$23).

## 8. Checklist de HANDOFF (fim da implantação — nada pode ficar de fora)

Hoje `terra1run9wz…26mawp` é owner E admin de tudo (verificado on-chain em
18/08/2026). Ao final da implantação, transferir:

### Terra Classic → módulo de governança (`terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n` — confirmado no guia oficial do deploy)
- [ ] `owner` do Mailbox, ISM multisig, IGP e IGP-oracle (Ownable, 2 passos:
      init pelo deployer + claim via proposta)
- [ ] `owner` do relayer-reward-vault e do oracle-governor (UpdateConfig/SetOwner)
- [ ] **`admin` (migrate) de TODOS os contratos** → gov ou `--no-admin`
      (o admin é o que permite migrate silencioso — não esquecer!)
- [ ] posse do StorageGasOracle já deve estar no oracle-governor (Fase 1)

### BSC / Ethereum → multisig dos validadores (3 val do TC + 1 não-validador — modelo aprovado; threshold: ver §6)
- [ ] `owner` do Vault e do GasOracleGovernor (2 passos: transferOwnership + acceptOwnership)
- [ ] `owner` do IGP, do ISM e do StorageGasOracle→governor
- [ ] proxy admin / upgrade rights dos contratos Hyperlane, se upgradeable

### Solana → multisig
- [ ] `TransferIgpOwnership` → config PDA do governor (com teste em devnet antes)
- [ ] **upgrade authority dos programas** rrv e igp-oracle-governor → multisig
- [ ] multisig do governor (`SetMultisig`) apontando para a conta multiassinatura real


A proposta de governança final = este handoff + os parâmetros das seções 2–7.
