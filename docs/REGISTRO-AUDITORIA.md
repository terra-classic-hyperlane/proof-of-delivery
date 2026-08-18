# Registro Consolidado de Auditoria — proof-of-delivery (4 redes)

**Snapshot:** 18/08/2026 · sistema ATIVO nas 4 redes. Detalhes por rede:
`AUDITORIA-TC.md` · `AUDITORIA-BSC.md` · `AUDITORIA-ETH.md` · `AUDITORIA-SOLANA.md`.
Warp/validadores: `WARP-IGORFAKE.md`. Como operar cada contrato: `OPERACAO-CONTRATOS.md`.

## 1. Terra Classic (domain 132556) — lado COLATERAL

### Nossos contratos (fonte: `contracts/`, build reproduzível optimizer 0.17.0)

| Contrato | Endereço | code_id | SHA-256 (= data_hash on-chain) |
|---|---|---|---|
| **relayer-reward-vault** | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` | 11588 | `c9699711a661607bebe30819ee1dc0035ff5276523dbb08b80a108fb03721d82` |
| **oracle-governor** | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` | 11587 | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` |

### Infra Hyperlane (pré-existente, verificada byte a byte na Fase 0)

| Peça | Endereço |
|---|---|
| Mailbox (code_id 11371) | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` |
| IGP (beneficiary = **vault** ✓) | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP oracle (owner = **governor** ✓) | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` |
| Warp IGORFAKE (colateral) | `terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel` |
| Token cw20 | `terra1lpkaaqjaq8zfwktge3vy0zg46nxxsynsge2wxa7addpweu2w6gmsy3lhkr` |
| Módulo de governança (alvo do handoff) | `terra10d07y265gmmuvt4z0w9aw880jnsr700juxf95n` |

### Papéis e parâmetros vigentes

- **Owner (vault + governor):** `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`
  (deployer/relayer — TEMPORÁRIO até o handoff → governança).
- **Operador de preço:** `terra1run9wz…26mawp` · quórum **1**.
- Tarifa 50 LUNC/entrega · pool semeado 5.000 LUNC (100 claims) · época 6 h · delta 20%.
- ISMs de ENTRADA (validadores oficiais Hyperlane): ETH **6-de-9** · BSC **4-de-6** · SOL **3-de-5**.

## 2. BSC (domain 56) — sintético

| Peça | Endereço |
|---|---|
| **RelayerRewardVault** | `0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D` |
| **GasOracleGovernor** | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` |
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (beneficiary = **vault** ✓) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (owner = **governor** ✓) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |
| Warp IGORFAKE | `0x3605D8946FC6F5A75d89d92173100F59743B5318` |
| ISM (threshold 1) | `0xa82087B8eea0394B1476f716B91c10531025Ef42` |
| Validador do ISM | `0x71B2B8C36a0C76b74Be92eb7915E26A69b3B03eB` |

- **Owner (vault+governor) e operador único (quórum 1):** `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` (= relayer BSC).
- Tarifa 0,00005 BNB · janela 1.600.000 blocos · faixas(132556): rate `[3015730·27141570]` · gas `[3333333333·30000000000]`.
- ⚠️ Pool: **0** (semente pendente).

## 3. Ethereum (domain 1) — sintético

| Peça | Endereço |
|---|---|
| **RelayerRewardVault** | `0xDf90d3b7FF98466E148B334128374807b3e89EbD` |
| **GasOracleGovernor** | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` |
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (beneficiary = **vault** ✓) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle (owner = **governor** ✓) | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |
| Warp IGORFAKE | `0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5` |
| ISM (threshold 1, mesmo validador da BSC) | `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b` |

- **Owner e operador único (quórum 1):** `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` (= relayer ETH).
- Tarifa 0,0004 ETH · janela 100.800 blocos · faixas(132556): rate `[8861692·79755234]` · gas `[3333333333·30000000000]`.
- ⚠️ Pool: **0** (semente pendente).

## 4. Solana (domain 1399811149) — sintético

| Peça | Endereço |
|---|---|
| **pod program** (vault+governor FUNDIDOS; 1º byte roteia 0=vault/1=governor) | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` |
| rrv config PDA (**o POOL**; beneficiary do IGP ✓) | `Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w` |
| gov config PDA (**owner do IGP** ✓) | `4sZAfqDqEmR7LMWjrdNmoEkv8S6BDdnDkh5mfADenaaA` |
| IGP program | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` |
| IGP account inner (recebe pagamentos) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| Overhead IGP (referenciado pelo warp) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| Mailbox | `E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi` |
| Warp IGORFAKE | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` |
| Mint sintético | `CeLHx5Wm9AzuWRnP4URMfNqNa9kDDrnsNGoATCS96QwD` |
| ISM program | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` |

- **Upgrade authority do pod + multisig do governor:** `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (deployer).
- **Operadores:** `BirXd4Q…` e `PbEo7Fn2…` (relayer, registrado 18/08) · **quórum 1**.
- Tarifa 0,003 SOL · época 21.600 s · quórum do vault **1** (2→1 via proposta 2-de-2, 18/08) · faixas(132556): rate `[9800000000·88200000000]` ·
  gas `[9441·84975]` · decimals 6 · pool **0,3 SOL** ✓.
- `pod.so` 184.904 bytes, deploy `--max-len` exato (custo 1,359 SOL + finalize; rent recuperável).

## 5. Relayer em operação (1 operador nesta fase)

| Chain | Endereço do relayer |
|---|---|
| Terra Classic | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |
| BSC | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |
| Ethereum | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |
| Solana | `PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS` |

## 6. Estado de centralização ATUAL e alvo do handoff

Hoje (fase de implantação): o deployer de cada rede acumula owner + operador +
relayer. **Alvo aprovado pela governança**: TC → módulo de governança;
BSC/ETH/Solana → multisig dos validadores (3 validadores do TC + 1 não-validador;
threshold em definição — ver §8 de `PARAMETROS_PROPOSTA.md`). Procedimentos
passo a passo em `OPERACAO-CONTRATOS.md` §5.

## 7. Verificação rápida (uma linha por invariante)

```bash
NODE=https://rpc.terra-classic.hexxagon.io
# TC: oracle owner = governor · IGP beneficiary = vault · solvência
terrad q wasm contract-state smart terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d '{"ownable":{"get_owner":{}}}' --node $NODE
terrad q wasm contract-state smart terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz '{"igp":{"beneficiary":{}}}' --node $NODE
terrad q wasm contract-state smart terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q '{"solvency":{}}' --node $NODE
# BSC / ETH: oracle owner = governor · IGP beneficiary = vault
cast call --rpc-url https://bsc-dataseed.bnbchain.org 0x7dE950f8F0a037783989a6BE84B3620916552306 "owner()(address)"
cast call --rpc-url https://bsc-dataseed.bnbchain.org 0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923 "beneficiary()(address)"
cast call --rpc-url https://ethereum-rpc.publicnode.com 0x3987cCE8f08037EBF93Ef3a934753540A94196cE "owner()(address)"
cast call --rpc-url https://ethereum-rpc.publicnode.com 0x9650F1f8DB492750323172145e67Df4e89E964Aa "beneficiary()(address)"
# Solana: owner/beneficiary embutidos no account do Igp (FPTvDso…) — offsets 43/75
# (script de conferência: deploy/solana-init.mjs lê e valida; ver AUDITORIA-SOLANA.md)
solana balance Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w -u https://api.mainnet-beta.solana.com
```

## 8. Reprodutibilidade dos binários

- CosmWasm: `cosmwasm/optimizer:0.17.0` sobre este repo → sha256 idênticos aos
  data_hash on-chain (11587/11588). `artifacts/checksums.txt`.
- EVM: solc 0.8.22 `via_ir` (foundry.toml versionado) — `forge build` + comparar
  `cast code` com o deployed bytecode.
- Solana: `cargo build-sbf` (workspace pinado, Cargo.lock versionado,
  opt-level=z) → `pod.so` 184.904 bytes.
