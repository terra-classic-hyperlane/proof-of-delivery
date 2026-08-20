# Tarifas IGP e recompensas — modelo pass-through (aplicado 20/08/2026)

Modelo: **quem envia paga a tarifa no IGP da origem (~US$ 0,08) e a recompensa do
operador espelha a tarifa do corredor** — sem valor fixo próprio ("o que o usuário
pagou vai pro operador; o lucro dele é o que sobra depois do gás que ele gasta").
A tarifa flutua com gás/câmbio reais (oracle-agent atualiza os oracles a cada 4h),
então o valor em $ deriva com o mercado; **para recentrar em $0,08, rode o script
de novo** (idempotente).

Ferramenta: `deploy/igp-tariff.mjs`
```bash
DRY=1 node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol   # conferir
TC_PRIVATE_KEY=… BSC_PRIVATE_KEY=0x… ETH_PRIVATE_KEY=0x… \
  node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol      # aplicar
# TARGET_USD=0.10 muda o alvo (padrão 0.08)
```

Preços usados nesta aplicação: **LUNC $0,00005051 · BNB $625,11 · SOL $84,84 ·
ETH $2.256,75** · alvo **$0,08/envio**.

> O "gás cobrado"/overhead é só **unidade de cobrança** (quote = gás × gas_price ×
> exchange_rate do oracle) — o relayer não gasta esse gás; ele paga o gás real da
> entrega no destino.

## Recibos pagam GÁS REAL, não a tarifa (migrate de 20/08/2026)

### Onde mora o markup de $0,08 em cada chain (por que só o TC precisou de fix)

| Chain | Onde está a tarifa | Quem passa por ela |
|---|---|---|
| BSC / ETH | **IGP custom** (`TerraClassicIGPStandalone`) — hook **só do warp** | só transferências de usuário |
| Solana | **overhead IGP** (`FXacR73…`) — **só o warp** referencia | só transferências de usuário |
| **TC** | **IGP compartilhado** (default hook do mailbox) | **tudo** que sai do TC — warp E recibos |

Nas remotas os recibos usam o **mailbox oficial** da chain (hook oficial, barato) —
nunca tocam no nosso IGP custom. Prova on-chain: o recibo TC→BSC emitido em
20/08 APÓS a tarifa nova (tx `0x445eda568614871322c067757f6a996b554b1accdbbed656e263f4f76e5a95a9`)
pagou **value = 0** + só o gás da tx (~$0,006).

No TC, sem ajuste, os RECIBOS também pagariam $0,08 e devorariam a comissão
(BSC→TC e SOL→TC ficavam **negativos** — P&L medido: −$0,008 e −$0,002).
Correção (code_id **11596**, migrate `53ACFEC1…`, mesmo endereço, pool
preservado, `layout_check ok`): `SendReceipt{gas_limit}` — o vault passa
metadata ao IGP (32B BE do gás + refund vazio → refund = o pool) e o recibo
paga só o gás real de entrega. O warp não expõe metadata → usuário segue
pagando $0,08, sem furo na tarifa. O claim-agent cota o IGP dinamicamente
(`quote_gas_payment` do `gas_limit`, +2% de folga; env `RECEIPT_GAS_56` /
`RECEIPT_GAS_SOL`) — sem valores fixos. Migração:
`deploy/tc-migrate-vault-gas-recibo.sh`.

### Mapa dos recibos por corredor (pós-migrate)

| Corredor | Recibo sai de | IGP que cobra | Custo do recibo |
|---|---|---|---|
| TC→BSC | BSC (vault `0x34E06a…`) | mailbox oficial da BSC | $0 + gás ~$0,006 ✓ provado |
| TC→ETH | ETH (vault ainda não existe) | mailbox oficial da ETH | idem, quando existir |
| BSC→TC | TC (vault `terra1gqkrh2…`) | nosso IGP, gás 300k via metadata | ~100 LUNC (~$0,005) |
| SOL→TC | TC (vault `terra1gqkrh2…`) | nosso IGP, gás 500k via metadata | ~20 LUNC (~$0,001) |
| TC→SOL | sem recibo (quórum de épocas na Solana) | — | ~$0,0001 (tx do relatório) |

**P&L do operador por transferência** (preços de 20/08): TC→SOL +$0,079 ·
TC→BSC +$0,065 · SOL→TC +$0,075 · BSC→TC +$0,067 — todos os corredores
lucrativos e automáticos (sem lógica de "só enviar quando compensa").

---

## Terra Classic (columbus-5, domain 132556) — origem TC→ETH/BSC/SOL

| Peça | Endereço |
|---|---|
| **IGP** | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP Oracle (StorageGasOracle) | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` (owner = oracle-governor `terra1z7jmlky…9sv4hj`) |
| Vault (recompensas) | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` |
| Owner (IGP/vault) | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |

**Tarifa** — `SetGasForDomain` tx `91883150EC580BDA624B0EBA0BBD9B8365D2C24CDA6352216DB46AC8FE226402`
(o `default_gas` segue 100.000; formato cw: `u128` em JSON é **string**):

| Destino | Gás cobrado (antes → agora) | Quote na aplicação |
|---|---|---|
| 1 (Ethereum) | 100.000 → **5.394.480** | ≈ 1.583,8 LUNC = $0,08 |
| 56 (BSC) | 100.000 → **4.803.897** | ≈ 1.583,8 LUNC = $0,08 |
| 1399811149 (Solana) | 100.000 → **40.158.741** | ≈ 1.583,8 LUNC = $0,08 |

**Recompensas** (vault, pagas em LUNC do pool TC — espelham a tarifa):

| Knob | Valor | tx |
|---|---|---|
| `remote_reward[1]` (entregas TC→ETH) | **1.583.844.760 uluna** ($0,08) | `6D8CB3A1E8BBF6CB5E123ACB4CFAFB3ECCF9CBCED3502EEC2B32448287248726` |
| `remote_reward[56]` (entregas TC→BSC) | **1.583.844.841 uluna** ($0,08) | `43970286BFB43FC146227DBBFB87764BB9706A1C52E19479CCB908E4F3DE11C2` |
| `reward_per_delivery` (claim direto) | 1 uluna (desativado de propósito — modelo de recibo cobre) | — |

## BSC (domain 56) — origem BSC→TC

| Peça | Endereço |
|---|---|
| **IGP** (TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` (owner = GasOracleGovernor `0x5CF7A3a7…13E5`) |
| Vault de recibo (recompensas) | `0x34E06a7793877EC5251b1dC230aD7cD577d231f4` |
| Owner | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

- **Tarifa**: `gasOverhead` 200.000 → **14.159.690** — tx `0x424e95c65d9ec56139bf3f230ef51917fd5a0f7c93c496b6748716e52dc06042` · quote verificada após: **$0,08**.
- **Recompensa**: `remoteReward[132556]` (entregas BSC→TC, paga em BNB) = **127.977.472.971.950 wei** (0,000128 BNB = $0,08) — tx `0xfd57684606ed28f43f7c19d3092a16a7652265d5b543e19c295cc9dc6f111992`.
- `rewardPerDelivery` (claim direto) segue 5e13 wei (0,00005 BNB) — não alterado.

## Ethereum (domain 1) — origem ETH→TC

| Peça | Endereço |
|---|---|
| **IGP** (TerraClassicIGPStandalone) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` (owner = GasOracleGovernor `0xa1803b36…fEFA`) |
| Owner | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

- **Tarifa**: `gasOverhead` 300.000 → **1.469.432** — tx `0x50a2fcdc224804997b2b31562e8c0eb75d83bab2b865b68c41f80d245196c42b` · quote verificada após: **$0,08**.
- **Recompensa ETH→TC**: o vault de recibo do ETH **ainda não foi deployado** (aguardando
  gás baixo). A recompensa das entregas **TC→ETH** já é paga no TC (`remote_reward[1]`).
  Quando o vault ETH existir: rodar `igp-tariff.mjs --rewards --eth` (e incluir o vault no script).

## Solana (domain 1399811149) — origem SOL→TC

| Peça | Endereço |
|---|---|
| **IGP program** | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` |
| IGP inner (acumula os lamports; beneficiary/owner do proof-of-delivery) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| **Overhead IGP** (o que o warp usa) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| pod (vault+governor, recompensas) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` (rrv config/pool `Eq1mJGTS…wb9w`) |
| Owner / authority | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

- **Tarifa**: `SetDestinationGasOverheads(132556)` 3.000.000 → **8.660.148** (gás intrínseco do warp ~3M → total ~11,66M) — tx `gqPF86BQetpApYz7VWpX5MHSEBvJXJqnPQAoNxcefuh3PkBERM2hiiFi5PN9SQyrtXTqHTerairGgKxLhadjUFa`.
- **Recompensas** (pagas em SOL do pool do pod):

| Knob | Valor | tx |
|---|---|---|
| `SetRemoteReward(132556)` (entregas Solana→TC, recibo) | **942.951 lamports** ($0,08) | `4p5XwNvf3sLmL2Yms6nvLsaUUi5w4gGakiyYF6UTinyVNnq2itS5Fkx5WWSar6kcZ9d5GLrUf5tQJ6o7T7QAqCM6` |
| `SetRewardLamports` (entregas TC→Solana, quórum de épocas) | **942.951 lamports** ($0,08) | `2BGncqeHeuHNmz7QdAikvays4upfALvHeeGVSBEfVbGTcAXzNC3qdp3gbekRPc1u26v3BiTnnHAMTf1pNqCyiPWS` |

---

## Resumo por corredor (na aplicação, 20/08/2026)

| Corredor | Remetente paga (origem) | Operador recebe (onde) |
|---|---|---|
| TC→ETH | ~1.584 LUNC ($0,08) | 1.583.844.760 uluna no TC |
| TC→BSC | ~1.584 LUNC ($0,08) | 1.583.844.841 uluna no TC |
| TC→Solana | ~1.584 LUNC ($0,08) | 942.951 lamports na Solana |
| BSC→TC | 0,000128 BNB ($0,08) | 127.977.472.971.950 wei no BSC |
| ETH→TC | ~0,0000354 ETH ($0,08) | (vault ETH pendente) |
| Solana→TC | ~942.951 lamports ($0,08) | 942.951 lamports na Solana |

**Manutenção**: reajustar quando o $ derivar (ex.: mensal, ou se algum token mover >2×):
`node igp-tariff.mjs --tariff --rewards --tc --bsc --eth --sol`. O pool fica neutro nos
corredores de mesmo token (arrecada X, paga X); no cruzado TC↔Solana a conversão usa o
câmbio do dia e os pools absorvem pequenas derivas.
