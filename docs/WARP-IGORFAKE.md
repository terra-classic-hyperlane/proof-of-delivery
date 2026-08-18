# Warp Route IGORFAKE — mapa completo (on-chain, 18/08/2026)

Referência de todos os endereços, IGPs, oráculos, ISMs+validadores e preços do
warp **IGORFAKE** (colateral cw20 no Terra Classic ↔ sintético em BSC/ETH/Solana).
Colhido direto da chain — as fontes de verdade são os contratos, não a config.

Token: **IGORFAKE** · 6 decimais · TC = colateral (cw20) · remotas = sintético.

---

## Terra Classic (columbus-5, domain **132556**) — LADO COLATERAL

| Peça | Endereço |
|---|---|
| **Warp (cw20 collateral router)** | `terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel` |
| cw20 do token (colateral real) | `terra1lpkaaqjaq8zfwktge3vy0zg46nxxsynsge2wxa7addpweu2w6gmsy3lhkr` |
| Mailbox | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` |
| IGP | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP Oracle | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` |
| ISM Routing | `terra1uhzzvt9x3u8hjnkp695hklexx2uywjvfqv454d93ds92sgtpwk7qrpxdg0` |
| Owner (todos) | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |

- cw20 `total_supply`: **999999999998810400** (≈ 10¹²·10⁶); colateral travado que lastreia os sintéticos.
- **IGP Oracle — preços vigentes (o que o usuário do TC paga por destino):**

| Domínio destino | exchange_rate | gas_price |
|---|---|---|
| 1 (Ethereum) | 376 | 10000000000 (10 gwei) |
| 56 (BSC) | 1098 | 3000000000 (3 gwei) |
| 1399811149 (Solana) | 383001553014 | 1 (modelo lamport) |

- Rotas enroladas (routers remotos, hex 32B): dom 1 `…a687a4c4…`, dom 56
  `…3605d894…`, dom 1399811149 `c6de5b1f…437f95`, dom 1399811151 (SOL testnet) `db7c50f8…`.

### ISM Multisig por domínio (validadores OFICIAIS da Hyperlane que assinam p/ o TC)

| ISM (para msgs vindas de) | Endereço | Threshold | Validators |
|---|---|---|---|
| Ethereum (dom 1) | `terra187rzjc3dznfxqtqqrwh796e5q4khmvp5av8mka6zhp98zjfk2z2qneldar` | **6 de 9** | ver guia de deploy TC |
| BSC (dom 56) | `terra1nqj7qlnt2sty0dgnu3ss5z4u6wr7hjfea7cn6wpwjt2uymts8ucsmuj9xw` | **4 de 6** | idem |
| Solana (dom 1399811149) | `terra10s3p36tjek8amhlc4krxpzln6g8n0qy9jq82wyda434l3rv89wfsucl50t` | **3 de 5** | idem |

> Esses ISMs verificam a ENTRADA no TC (mensagens vindas das remotas). O
> `proof-of-delivery` NÃO os altera — só passa a ler o Mailbox p/ pagar o relayer.

---

## BSC (domain **56**) — LADO SINTÉTICO

| Peça | Endereço |
|---|---|
| **Warp (synthetic router)** | `0x3605D8946FC6F5A75d89d92173100F59743B5318` |
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (custom TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |
| ISM (messageIdMultisig) | `0xa82087B8eea0394B1476f716B91c10531025Ef42` |
| Owner (warp/IGP/oracle/beneficiary) | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

- `totalSupply` sintético: **3688000000** (3.688 IGORFAKE, 6 dec).
- **ISM validators:** `0x71B2B8C36a0C76b74Be92eb7915E26A69b3B03eB` · **threshold 1**
  (multisig do próprio operador — valida msgs vindas do TC).
- **Oracle (o que o usuário da BSC paga para mandar ao TC):** rate `9047190` · gas_price `1e10`.

---

## Ethereum (domain **1**) — LADO SINTÉTICO

| Peça | Endereço |
|---|---|
| **Warp (synthetic router)** | `0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5` |
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (custom) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |
| ISM (messageIdMultisig) | `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b` |
| Owner (warp/IGP/oracle/beneficiary) | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

- `totalSupply` sintético: **1308000000** (1.308 IGORFAKE).
- **ISM validators:** `0x71B2B8C36a0C76b74Be92eb7915E26A69b3B03eB` · **threshold 1**.
- **Oracle:** rate `26585078` · gas_price `1e10` (10 gwei).

---

## Solana (domain **1399811149**) — LADO SINTÉTICO

| Peça | Endereço |
|---|---|
| Warp/router (hex no TC) | `c6de5b1fd8d285c06fa3967440530edfec35e907464599e3b485c5f273437f95` |
| IGP program | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` |
| IGP account (inner) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| Overhead IGP account | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| ISM (MultisigISM program) | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` |
| Owner | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

- Oracle vive DENTRO do IGP (`gas_oracles` do Igp) — rate/gas do TC lidos on-chain
  no deploy (fallback: rate 2,94e10 · gas 28325 · decimals 6 · overhead 3e6).
- ISM threshold 1. Também existe rota p/ **Solana testnet** (dom 1399811151).

---

## Onde o proof-of-delivery se conecta

Em cada perna, o **IGP** desta tabela recebe `beneficiary = Vault` e o **oracle**
passa ao **governor** (owner). No TC isso JÁ ESTÁ FEITO (Fases 1–2 — ver
`AUDITORIA-TC.md`): IGP `terra1taunhg…` → beneficiary vault `terra1gqkrh2…`,
oracle `terra1j8xz…` → owner governor `terra1z7jmlky…`. Nas remotas, os scripts
`evm-deploy.sh`/`solana-deploy.sh` fazem o mesmo com os endereços acima.
