# Registro de Auditoria — Deploy Terra Classic (Fases 1–2)

**Data:** 2026-08-18 · **Chain:** columbus-5 · **Deployer:** `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`
**Código-fonte:** este repositório (github.com/terra-classic-hyperlane/proof-of-delivery)
**Build:** `cosmwasm/optimizer:0.17.0` sobre locks versionados (reproduzível — ver README §Build)

## Códigos armazenados

| Contrato | code_id | SHA-256 (= data_hash on-chain, verificado no deploy) | TX do store |
|---|---|---|---|
| oracle-governor | **11587** | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` | `657F893FE4CDF0E20CAAF94D49348B23FC84F802FC0A6DC12B11EDCC38C6BB26` |
| relayer-reward-vault | **11588** | `c9699711a661607bebe30819ee1dc0035ff5276523dbb08b80a108fb03721d82` | `2DE362BA9D9D002A0D7DAD0D81A37CF91CE0F1FF147A3B324690AF51DC662CBE` |

## Contratos instanciados

| Contrato | Endereço | TX do instantiate |
|---|---|---|
| oracle-governor | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` | `31DB39EB87E106F3FA51ED4BEE0B81A82E0DA56944BDC02BA095E2F170B64E46` |
| relayer-reward-vault | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` | `6653EFCBF792FD10E357E0A4AD51E78954EC96317AC2E929A0AE1F5D1283F419` |

Parâmetros de instanciação: governor `{owner: deployer, oracle: terra1j8xzgzk…cwqj0d,
operators: [deployer], quorum: 1, epoch: 21600s, delta: 2000 bps}` · vault
`{owner: deployer, mailbox: terra1fwg35n…jpx3p9, igp: terra1taunhg…qqnvz,
denom: uluna, reward: 50000000 (50 LUNC), janela: 200000 blocos}`.

## Transações de configuração

| # | Ação | TX |
|---|---|---|
| 1 | StorageGasOracle: `init_ownership_transfer` → governor | `31B0DF7E4DA2CE64F71EC15C328B233D7EA0E179D420557E5D1DB2AB7189CDA7` |
| 2 | governor: `claim_oracle_ownership` (posse efetivada) | `EDE721139C5C205924FF74BDA067D435A6EBEC8DD93F06F8929901F65A80BD97` |
| 3 | `set_bounds` dom 1 (Ethereum) — rate [125·1128] · gas [3333333333·30000000000] | `94268C079628CE722754AAB5BA3606FB1E3BE1BB151A69EF1E97A619E3E81BDB` |
| 4 | `set_bounds` dom 56 (BSC) — rate [366·3294] · gas [1000000000·9000000000] | `C6B7EB3DC78FC768BC19ACE0A54F4211CF4B82C9413DF0EDB59563C8735143D0` |
| 5 | `set_bounds` dom 1399811149 (Solana) — rate [127667184338·1149004659042] · gas [1·3] | `60934DB01C558450B905B32D1A4485D7EC27A44DD875166C6CBB7916155271FE` |
| 6 | IGP `set_beneficiary` → vault | `4895068D2D03BDC956136BC3E77E4F75FE8111D490E6ABF29E22865604121148` |
| 7 | Semente do pool: 5.000 LUNC (BankSend) → vault | `B55FD50BC7473F8D96B65B43A8879E78BB4676C66A6741D8A04463DF277844C7` |

Faixas derivadas dos valores VIGENTES no oracle de produção no momento do deploy
(vigente ÷3 · ×3) — vigentes lidos: dom 1 = 376·1e10 · dom 56 = 1098·3e9 ·
dom 1399811149 = 383001553014·1.

## Como auditar (qualquer pessoa)

```bash
NODE=https://terra-classic-rpc.publicnode.com:443

# 1. data_hash on-chain == sha256 desta tabela == rebuild do fonte:
terrad q wasm code-info 11587 --node $NODE   # → 3383E2BC…
terrad q wasm code-info 11588 --node $NODE   # → C9699711…
docker run --rm -v "$(pwd)":/code ... cosmwasm/optimizer:0.17.0 && cat artifacts/checksums.txt

# 2. cada TX acima:
terrad q tx <HASH> --node $NODE

# 3. estado vigente:
terrad q wasm contract-state smart terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d '{"ownable":{"get_owner":{}}}' --node $NODE           # owner do oracle = governor
terrad q wasm contract-state smart terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz '{"igp":{"beneficiary":{}}}' --node $NODE            # beneficiary = vault
terrad q wasm contract-state smart terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q '{"solvency":{}}' --node $NODE                        # pool/capacidade
terrad q wasm contract-state smart terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q '{"layout_check":{"message_id":"d039daa1c75d5b558906fef6d790b13dc94a8b39e58e1e7f219b3967a28c4f04"}}' --node $NODE
```

Verificação pós-deploy executada em 18/08/2026: owner do oracle = governor ✓ ·
beneficiary = vault ✓ · solvency = 5.000 LUNC / 100 claims ✓ · layout_check
`ok:true` (sender=terra1run9wz…, block 29422362) ✓.

> Deploys futuros (BSC/Ethereum/Solana — Fases 3–4) devem ganhar registros
> equivalentes: `AUDITORIA-BSC.md`, `AUDITORIA-ETH.md`, `AUDITORIA-SOL.md`.

## Vault v2 (ClaimRemote) — migração 19/08/2026

**Migração NO MESMO endereço** (`terra1gqkrh2…duzc2q`), assinada pelo admin
`terra1run9wz…`. Pool e beneficiary preservados. Build reproduzível
`cosmwasm/optimizer:0.17.0` → sha256 `e24a5e66ab4a503c6acf369710b717310362d2ae5fa7b9800542c8272b2fc801`.

| Passo | Tx |
|---|---|
| store (code_id **11589**) | `A9866AEE5A37F76BDFDF4A2FDE15B2AB3319715550954EDFACE0E75A9D61E76B` |
| migrate | `C4075BA84D9545EBD912B84A693968DFB2A4123391362930A0F3E2B663F03DAD` |
| set_remote_operators (atestador terra1run9wz…, quórum 1) | `322620CAB36F631B76AE2FB5A711CE8F7CADB8EEB86E45D563AD9CEE0BA6F821` |
| vínculo Solana → `PbEo7Fn2…cwwrkS` | `3526970A18F1A1C66565E42D7ABEB9ED95D991B1BB815ED462613790D40212BE` |
| vínculo BSC → `0x8f085bad…5291` | `51763033E7BE6166C881F45BB60D4A4FBBB6F522CAC214BBAECDE20EB6C07F05` |
| vínculo ETH → `0xef818120…00ae` | `04174C7EB2608C66AF0303B93B49573E499199369B5A485C4B38291498159908` |
| recompensa 33 LUNC dom SOL / BSC / ETH | `E7D9F83A…11DBC8` · `AFF7068C…6A2DA8` · `0CCF3BC3…3EAE9B` |
| **atesta entrega SOL** `1e070a74…` (+33 LUNC) | `844240A4154A672580A58F78EE5F33BEA751F5895DD52C2333851C3C2080CA95` |
| **atesta entrega BSC** `72f1099d…` (+33 LUNC) | `4F687166BB2A155F34A4558BFA034455BBD57D4B028EB7043B76CE784AC1921B` |
| **atesta entrega ETH** `6c6518b0…` (+33 LUNC) | `44E0B62DA58CF8ADCC87137AE6CAD3881127056C755AFD7A2DECE8731D9DF6FA` |

Verificação independente pós-migração (LCD, 19/08): code_id **11589** ✓ ·
`remote_claimed(1e070a74…)` = pago 33 LUNC ao executor `terra1run9wz…` no bloco
30011260 ✓ · vínculos ativos ✓ · `total_remote_paid` = 99 LUNC ✓ · pool
4.901 LUNC (5.000 − 99, pagamentos saíram do pool como projetado) ✓.
Modelo completo: `CLAIM-REMOTO.md`.
