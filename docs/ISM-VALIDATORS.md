# IGORFAKE warp ISMs — 3-of-4 validators (2026-08-20)

## CURRENT STATE (definitive) — MUTABLE ISMs on ETH/BSC

Still on 2026-08-20 the static ISMs from the migration below were replaced by
**StorageMessageIdMultisigIsm** (official upstream Hyperlane contract, PR #4577,
unchanged source) — mutable: **these addresses do NOT change anymore**; validator
rotation is an owner tx (`setValidatorsAndThreshold`), without touching warp/vault.
Tool: `deploy/storage-ism.mjs` (state in `deploy/storage-ism.state`).

| Chain | Definitive ISM | Factory (own) | Owner |
|---|---|---|---|
| **Ethereum** | `0x3ba17675f0D319C89D70722f6eb07790DF0B254B` | `0xCB8BC1921f1a2334f7c73D1299F94b97A10bc583` | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |
| **BSC** | `0xF6b0cDD33A7d2895a3F18b85569Ed9A8278cD151` | `0x10067DE13589c3A3380d1B72b04c9b45147A8112` | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |
| **Solana** | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` (always was mutable — unchanged) | — | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

Txs (2026-08-20): ETH factory `0x39eb8976…4250d2c` · ISM `0x2a6a7211…c84248c` ·
warp points to `0xfdd6b1a4…b3b5939` — BSC factory `0x299cf2f3…4a8eb18` · ISM
`0xce532cbe…59f70b1b` · warp `0x35f527eb…837fe161` · receipt vault
`0xf342418c…af206fc3f`. Where the ISM is configured: ETH warp, BSC warp, BSC receipt
vault (`0x34E06a…`), Solana pod (constant `WARP_ISM` in code = `4MzF7…`).
The ETH receipt vault (pending) is created with the new ISM from the start (`evm-vault-receipt.sh`
updated). Note: the **ZTT** warp (BSC `0x6AB3EaF4…`) stays on the old static ISM
`0xa820…` on purpose — on mainnet only IGORFAKE is in production.

**Future validator rotation** (addresses do not change):
```bash
# ETH/BSC: edit VALIDATORS/THRESHOLD at the top and
node storage-ism.mjs --set --eth --bsc
# Solana:
node update-ism-validators.mjs --sol
```

---

## History — migration 1-of-1 → 3-of-4 (static ISMs, same day, superseded)

Record of the migration of the **synthetic** ISMs (ETH, BSC, Solana) from
`[igorveras] / threshold 1` to the set of **4 validators with threshold 3**.
Executed on 2026-08-20 via `deploy/update-ism-validators.mjs`; state checked
on-chain after each transaction. The static ISMs `0xCB13…` (ETH) and `0xcA21…` (BSC)
from this step were orphaned hours later, replaced by the mutable ones above.

> These ISMs verify the messages **coming from Terra Classic (domain 132556)**
> on the synthetics. The ISMs on the TC side (entry of msgs from the remotes) use the
> official Hyperlane validators and were NOT changed — see archive/WARP-IGORFAKE.md (route discontinued 2026-08-29).

## Validator set (sign TC checkpoints)

| Name | Address (EVM signing key) |
|---|---|
| igorveras | `0x71b2b8c36a0c76b74be92eb7915e26a69b3b03eb` |
| tcv | `0x1afd3d07abd2aaa19a9f7993f334a926e253b90c` |
| darksun | `0xe6bb040164a0ebbcb7e2d584f066c8b57dd74383` |
| burnitall | `0x5c374754892ebac52702475726b67f822efdfacc` |

**Threshold: 3 of 4.** A TC→remote message is only accepted with ≥3 signatures.
⚠️ If fewer than 3 validators are active/signing, delivery **stops** — availability
now depends on the set, not only on igorveras.

## Ethereum (domain 1)

| Piece | Value |
|---|---|
| **New ISM (static messageIdMultisig)** | `0xCB133033D5091689f12d913C76a1477f2f5D0191` |
| Created by the factory | `staticMessageIdMultisigIsmFactory 0xfA21D9628ADce86531854C2B7ef00F07394B0B69` (CREATE2 — deterministic address for [4 validators]+3) |
| ISM deploy tx | `0x45f0ed4e2af33672615ea2d29d3da77bdca30f46a93cc945f3037e8206e0d5f0` |
| Warp (synthetic router) | `0xA687a4C4Ca49795999b36fDC8A18D1ddD63EdfB5` |
| `setInterchainSecurityModule` tx | `0x2de47c6d997857f785887a006a5cc6d39bb45acb8aa57e58fa7c966afcd15bf1` |
| Previous ISM (retired) | `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b` (static, [igorveras]/1) |
| Warp owner (signed) | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

## BSC (domain 56)

| Piece | Value |
|---|---|
| **New ISM (static messageIdMultisig)** | `0xcA21D04eE1B1155d8548391770E1DFE3D9adc661` |
| Created by the factory | `staticMessageIdMultisigIsmFactory 0x4B1d8352E35e3BDE36dF5ED2e73C24E35c4a96b7` |
| ISM deploy tx | `0x1a1c520e775b5894d55fb7c1f4f9a4fd8d645c75bb44c970984385456e4248f2` |
| Warp (synthetic router) | `0x3605d8946fC6f5a75D89D92173100F59743b5318` |
| `setInterchainSecurityModule` tx | `0x64e1f6c0d3407fc3593400a6ba6f93e9912d15edd8daea1b2d52b7e5c3bf0035` |
| Previous ISM (retired) | `0xa82087B8eea0394B1476f716B91c10531025Ef42` (static, [igorveras]/1) |
| Warp owner (signed) | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

## Solana (domain 1399811149)

The Solana ISM is **mutable** (program `multisig-ism-message-id`): the program is the
same — only the validators/threshold of domain 132556 were swapped via
`SetValidatorsAndThreshold`.

| Piece | Value |
|---|---|
| **ISM (program, unchanged)** | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` |
| domain_data PDA (dom 132556) | `7YypjZXNWQhRGJXr1TWZYaf4PdiFSmbbrANGieFUV1gJ` — now 4 validators / threshold 3 |
| access_control PDA (owner) | `3v1B25oUxUKSQAF69pbCxb8U8ZcSUEDUcwbB2ShiBE1r` → owner `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |
| `SetValidatorsAndThreshold` tx | `2gaPqn8TQLwfTW21XhiooJCqWzYWpqRzP5ndA9qmwjg36ry249bAZBdoMryJdMmDf5E1MrRJeanxzafsGMd28PyH` |
| Warp/router (program) | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` |

## How to change again in the future

`deploy/update-ism-validators.mjs` (edit `VALIDATORS`/`THRESHOLD` at the top):

```bash
cd ~/tc-proof-of-delivery/deploy
DRY=1 node update-ism-validators.mjs --eth --bsc --sol          # check
ETH_PRIVATE_KEY=0x… BSC_PRIVATE_KEY=0x… node update-ism-validators.mjs --eth --bsc --sol
```

- **ETH/BSC**: static ISMs have no owner — changing validators = the factory creates
  another ISM (new, deterministic address) and the warp is pointed to it
  (`setInterchainSecurityModule`, warp owner signs).
- **Solana**: same `SetValidatorsAndThreshold` instruction, owner `BirXd4Q…` signs
  (keypair in `/home/lunc/keys/`).
