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

Fonte: log de deploy `WARP-SOLANAMAINNET-IGORFAKE-BUFFER.txt` + `REFERENCE-IGP`
(09/07) + tx real de dispatch (abaixo).

| Peça | Endereço |
|---|---|
| **Warp/router (program)** | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` (hex `c6de5b1f…437f95` = a rota no TC) |
| Mint (token sintético) | `CeLHx5Wm9AzuWRnP4URMfNqNa9kDDrnsNGoATCS96QwD` |
| Mailbox (sealevel) | `E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi` |
| **IGP program** | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` (binário sha256 `4321c426…08c6d`) |
| IGP account (inner — RECEBE o pagamento) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| Overhead IGP (o que o WARP usa) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| ISM (MultisigISM program) | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` (mainnet ID `LwNfVYMDzAe5dCJgA5CipTZcT34Eyf74zLr81K91jxk`) |
| Owner / upgrade authority | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

- **RemoteGasData do TC (132556) no IGP:** exchange_rate `29400000000` · gas_price
  `28325` · token_decimals `6` · gas_overhead `3000000` (o deploy lê o VIGENTE
  on-chain; estes são os valores de referência de 09/07).
- ISM threshold 1. Há também rota p/ **Solana testnet** (dom 1399811151).

> ⚠️ Dois IGP accounts: o **inner `FPTvDso…`** é quem acumula os lamports (é ELE
> que recebe `beneficiary = vault` e `owner = governor`); o **overhead
> `FXacR73…`** é o que o warp referencia. O `solana-init.mjs` já usa `FPTvDso…`.

### Prova do ciclo completo (tx real)

`4wiG4TtZDFgvzY1wWYTkTDr8J64svrDZD4qShLYk5VQXKchEM1MCGoCw11X9ZyBzuZcztqFVHQymrChDX5Q6fpX1`
(slot 431826035): burn do sintético → `Dispatched message to 132556, ID
0xd039daa1c75d5b558906fef6d790b13d…` → `Paid IGP FPTvDsow… for 6000000 gas`.
Essa é **a mesma mensagem** que o relayer `terra1run9wz…` entregou no TC (bloco
29422362) e que o vault valida via `layout_check` (`ok:true`) — o fluxo
SOL→TC→prova-de-entrega fechado de ponta a ponta.

> Existe um deploy ANTERIOR (program `Behbk6ULj6PjvN9ZTb5vesyfor2hFhkw6y131UCRtgPx`,
> mint `HHVv9R48…`, IGP `BhNcatUDC2D…`, domain 1325/testnet) preservado nos logs —
> NÃO é o de produção. O de produção é o `EPJNrr…` acima.

---

## Onde o proof-of-delivery se conecta

Em cada perna, o **IGP** desta tabela recebe `beneficiary = Vault` e o **oracle**
passa ao **governor** (owner). No TC isso JÁ ESTÁ FEITO (Fases 1–2 — ver
`AUDITORIA-TC.md`): IGP `terra1taunhg…` → beneficiary vault `terra1gqkrh2…`,
oracle `terra1j8xz…` → owner governor `terra1z7jmlky…`. Nas remotas, os scripts
`evm-deploy.sh`/`solana-deploy.sh` fazem o mesmo com os endereços acima.
