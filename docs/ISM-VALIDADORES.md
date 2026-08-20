# ISMs do warp IGORFAKE — validadores 3-de-4 (20/08/2026)

## ESTADO ATUAL (definitivo) — ISMs MUTÁVEIS na ETH/BSC

Ainda em 20/08/2026 os ISMs estáticos da migração abaixo foram substituídos por
**StorageMessageIdMultisigIsm** (contrato upstream oficial da Hyperlane, PR #4577,
fonte inalterada) — mutáveis: **estes endereços NÃO mudam mais**; rotação de
validador é uma tx do owner (`setValidatorsAndThreshold`), sem tocar em warp/vault.
Ferramenta: `deploy/storage-ism.mjs` (estado em `deploy/storage-ism.state`).

| Chain | ISM definitivo | Factory (própria) | Owner |
|---|---|---|---|
| **Ethereum** | `0x3ba17675f0D319C89D70722f6eb07790DF0B254B` | `0xCB8BC1921f1a2334f7c73D1299F94b97A10bc583` | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |
| **BSC** | `0xF6b0cDD33A7d2895a3F18b85569Ed9A8278cD151` | `0x10067DE13589c3A3380d1B72b04c9b45147A8112` | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |
| **Solana** | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` (sempre foi mutável — inalterado) | — | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

Txs (20/08/2026): ETH factory `0x39eb8976…4250d2c` · ISM `0x2a6a7211…c84248c` ·
warp aponta `0xfdd6b1a4…b3b5939` — BSC factory `0x299cf2f3…4a8eb18` · ISM
`0xce532cbe…59f70b1b` · warp `0x35f527eb…837fe161` · vault de recibo
`0xf342418c…af206fc3f`. Onde o ISM está configurado: warp ETH, warp BSC, vault de
recibo BSC (`0x34E06a…`), pod Solana (constante `WARP_ISM` no código = `4MzF7…`).
O vault de recibo do ETH (pendente) já nasce com o ISM novo (`evm-vault-receipt.sh`
atualizado). Obs.: o warp **ZTT** (BSC `0x6AB3EaF4…`) segue no ISM estático antigo
`0xa820…` de propósito — em mainnet só o IGORFAKE está em produção.

**Rotação futura de validadores** (endereços não mudam):
```bash
# ETH/BSC: edite VALIDATORS/THRESHOLD no topo e
node storage-ism.mjs --set --eth --bsc
# Solana:
node update-ism-validators.mjs --sol
```

---

## Histórico — migração 1-de-1 → 3-de-4 (ISMs estáticos, mesmo dia, superados)

Registro da migração dos ISMs dos **sintéticos** (ETH, BSC, Solana) de
`[igorveras] / threshold 1` para o conjunto de **4 validadores com threshold 3**.
Executado em 20/08/2026 via `deploy/update-ism-validators.mjs`; estado conferido
on-chain após cada transação. Os ISMs estáticos `0xCB13…` (ETH) e `0xcA21…` (BSC)
desta etapa ficaram órfãos horas depois, substituídos pelos mutáveis acima.

> Estes ISMs verificam as mensagens **vindas do Terra Classic (domain 132556)**
> nos sintéticos. Os ISMs do lado TC (entrada de msgs das remotas) usam os
> validadores oficiais da Hyperlane e NÃO foram alterados — ver `WARP-IGORFAKE.md`.

## Conjunto de validadores (assinam checkpoints do TC)

| Nome | Endereço (signing key EVM) |
|---|---|
| igorveras | `0x71b2b8c36a0c76b74be92eb7915e26a69b3b03eb` |
| tcv | `0x1afd3d07abd2aaa19a9f7993f334a926e253b90c` |
| darksun | `0xe6bb040164a0ebbcb7e2d584f066c8b57dd74383` |
| burnitall | `0x5c374754892ebac52702475726b67f822efdfacc` |

**Threshold: 3 de 4.** Uma mensagem TC→remota só é aceita com ≥3 assinaturas.
⚠️ Se menos de 3 validadores estiverem ativos/assinando, a entrega **para** — a
disponibilidade agora depende do conjunto, não só do igorveras.

## Ethereum (domain 1)

| Peça | Valor |
|---|---|
| **ISM novo (messageIdMultisig estático)** | `0xCB133033D5091689f12d913C76a1477f2f5D0191` |
| Criado pela factory | `staticMessageIdMultisigIsmFactory 0xfA21D9628ADce86531854C2B7ef00F07394B0B69` (CREATE2 — endereço determinístico p/ [4 validadores]+3) |
| tx deploy do ISM | `0x45f0ed4e2af33672615ea2d29d3da77bdca30f46a93cc945f3037e8206e0d5f0` |
| Warp (synthetic router) | `0xA687a4C4Ca49795999b36fDC8A18D1ddD63EdfB5` |
| tx `setInterchainSecurityModule` | `0x2de47c6d997857f785887a006a5cc6d39bb45acb8aa57e58fa7c966afcd15bf1` |
| ISM anterior (aposentado) | `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b` (estático, [igorveras]/1) |
| Owner do warp (assinou) | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

## BSC (domain 56)

| Peça | Valor |
|---|---|
| **ISM novo (messageIdMultisig estático)** | `0xcA21D04eE1B1155d8548391770E1DFE3D9adc661` |
| Criado pela factory | `staticMessageIdMultisigIsmFactory 0x4B1d8352E35e3BDE36dF5ED2e73C24E35c4a96b7` |
| tx deploy do ISM | `0x1a1c520e775b5894d55fb7c1f4f9a4fd8d645c75bb44c970984385456e4248f2` |
| Warp (synthetic router) | `0x3605d8946fC6f5a75D89D92173100F59743b5318` |
| tx `setInterchainSecurityModule` | `0x64e1f6c0d3407fc3593400a6ba6f93e9912d15edd8daea1b2d52b7e5c3bf0035` |
| ISM anterior (aposentado) | `0xa82087B8eea0394B1476f716B91c10531025Ef42` (estático, [igorveras]/1) |
| Owner do warp (assinou) | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

## Solana (domain 1399811149)

O ISM da Solana é **mutável** (programa `multisig-ism-message-id`): o programa é o
mesmo — só os validadores/threshold do domínio 132556 foram trocados via
`SetValidatorsAndThreshold`.

| Peça | Valor |
|---|---|
| **ISM (program, inalterado)** | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` |
| domain_data PDA (dom 132556) | `7YypjZXNWQhRGJXr1TWZYaf4PdiFSmbbrANGieFUV1gJ` — agora 4 validadores / threshold 3 |
| access_control PDA (owner) | `3v1B25oUxUKSQAF69pbCxb8U8ZcSUEDUcwbB2ShiBE1r` → owner `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |
| tx `SetValidatorsAndThreshold` | `2gaPqn8TQLwfTW21XhiooJCqWzYWpqRzP5ndA9qmwjg36ry249bAZBdoMryJdMmDf5E1MrRJeanxzafsGMd28PyH` |
| Warp/router (program) | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` |

## Como alterar de novo no futuro

`deploy/update-ism-validators.mjs` (edite `VALIDATORS`/`THRESHOLD` no topo):

```bash
cd ~/tc-proof-of-delivery/deploy
DRY=1 node update-ism-validators.mjs --eth --bsc --sol          # confere
ETH_PRIVATE_KEY=0x… BSC_PRIVATE_KEY=0x… node update-ism-validators.mjs --eth --bsc --sol
```

- **ETH/BSC**: ISMs estáticos não têm owner — mudar validadores = a factory cria
  outro ISM (endereço novo, determinístico) e o warp é apontado pra ele
  (`setInterchainSecurityModule`, owner do warp assina).
- **Solana**: mesma instrução `SetValidatorsAndThreshold`, owner `BirXd4Q…` assina
  (keypair em `/home/lunc/keys/`).
