# tc-proof-of-delivery

Remuneração de múltiplos relayers na ponte Hyperlane (Terra Classic · BSC · Ethereum · Solana).
Especificação: `SPEC.html` (v3) · **Arquitetura com diagramas**: `docs/ARQUITETURA.md` · **Instalação/execução**: `docs/INSTALACAO_E_EXECUCAO.md` · Skills do repo em `.claude/skills/` (tc-pod-contratos · tc-pod-deploy · tc-pod-oracle-agent). Cada rede tem um Vault beneficiary do IGP local; o operador
recebe pelo que ENTREGOU, provado pelo registro de execução da própria chain (TC via raw query
do storage, EVM via processor(), Solana via quórum de épocas).

## Verificação de código-fonte (Fase 0 parcial — 18/08/2026)

Confirmado nos repositórios locais (`~/tc-cw-hyperlane` e `~/hyperlane-monorepo`):

| Afirmação da spec | Onde | Status |
|---|---|---|
| `Delivery { sender, block_number }` gravado no process() | tc-cw-hyperlane `mailbox/src/state.rs:50` + `execute.rs:191` | ✅ |
| Query pública usa `.has()` e descarta o executor | `mailbox/src/query.rs:56` | ✅ |
| Prefixo do Map = `"deliveries"` (10 bytes → chave raw `[0x00,0x0A]+"deliveries"+id`) | `state.rs:64` | ✅ |
| `claim()` do IGP CosmWasm só aceita o beneficiary | `igps/core/src/execute.rs:90-92` | ✅ (Sweep necessário) |
| EVM `Delivery { processor, blockNumber }` + `processor(id)` + `processedAt(id)` | `solidity/contracts/Mailbox.sol:55,253,262` | ✅ |
| `claim()` do IGP EVM é permissionless (paga sempre ao beneficiary) | `hooks/igp/InterchainGasPaymaster.sol:142` | ✅ |
| Solana `ProcessedMessage` SEM campo de executor | `sealevel/programs/mailbox/src/accounts.rs:260` | ✅ |
| `Igp { owner, beneficiary, gas_oracles: HashMap }` — oracle dentro do IGP | `hyperlane-sealevel-igp/src/accounts.rs:159-169` | ✅ |
| `set_gas_oracle_configs` exige owner signer | `hyperlane-sealevel-igp/src/processor.rs:637` (+ ensure_owner_signer) | ✅ |

## Fase 0 — data_hash: FECHADA ✅ (18/08/2026)

O `data_hash` de **TODOS os contratos implantados** no Terra Classic confere byte a byte com os
artefatos staged do repositório de deploy (`tc-cw-hyperlane/tmp/codes/*.wasm`):

| Contrato | code_id | Verificação |
|---|---|---|
| hpl_mailbox | 11371 | ✅ `B6D789C1A31EE79548FD736BAD241DBCD3B8B319D66A776F31479743FE49EB01` |
| hpl_validator_announce | 11372 | ✅ |
| hpl_ism_multisig / routing | 11374 / 11376 | ✅ |
| hpl_igp / hpl_igp_oracle | 11377 / 11388 | ✅ |
| hooks (aggregate/merkle/pausable/fee) | 11378–11381 | ✅ |
| hpl_warp_cw20 | 11389 | ✅ |

Evidência forense adicional: o wasm on-chain embute o rustc `cc66ad46…` = **1.73.0 musl**, o
toolchain exato do `cosmwasm/optimizer:0.15.0` do Makefile — e o fonte dos contratos não é
alterado desde jan/2025. Cadeia completa: fonte verificado ➝ artefato staged idêntico ao
on-chain ➝ comportamento confirmado em mainnet (raw query do DELIVERIES).

Nota de reprodutibilidade: o `Cargo.lock` da época do deploy não foi versionado no
tc-cw-hyperlane (o lock atual, regenerado por cargo novo, resolve crates edition2024 que o
cargo 1.73 nem compila). Para futuros deploys DESTE repositório os locks são versionados —
o rebuild-from-source independente do upstream fica como exercício para auditoria externa.

## Fase 0 — evidência de MAINNET (18/08/2026) ✅

Raw query no estado do Mailbox em produção (`terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9`,
code_id 11371, label `cw-hpl: hpl_mailbox`, via LCD Hexxagon — o publicnode bloqueia o endpoint /state):

- `nonce` = 13 (mensagens despachadas do TC) · `latest_dispatch_id` = 26096daa…3220
- default_ism = terra1uhzzvt9x3u8hjnkp695hklexx2uywjvfqv454d93ds92sgtpwk7qrpxdg0
- **2 entradas DELIVERIES** decodificadas com sucesso pela chave bruta `[0x00,0x0A]+"deliveries"+message_id`:

| message_id | delivery (valor bruto decodificado) |
|---|---|
| d039daa1…4f04 | `{"sender":"terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp","block_number":29422362}` |
| d5e2ab02…cc4f | `{"sender":"terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp","block_number":29423109}` |

Conclusão: o wasm EM PRODUÇÃO grava `{sender, block_number}` exatamente como o repo local — a prova
por raw query do Vault é viável na mainnet. (Falta só o rigor final: comparar o data_hash do code_id
11371 com o build do repo.) Ambos os deliveries foram do relayer terra1run9wz…26mawp.

## Artefatos

| Artefato | Estado |
|---|---|
| `contracts/relayer-reward-vault` (CosmWasm) | ✅ escrito · **24 testes passando** (4 unit + 20 integração cw-multi-test) · clippy -D warnings limpo · compila p/ wasm32 |
| `contracts/oracle-governor` (CosmWasm) | ✅ escrito · **15 testes de integração** passando · clippy -D warnings limpo · compila p/ wasm32 |
| `evm/src/RelayerRewardVault.sol` + `evm/src/GasOracleGovernor.sol` | ✅ escritos · **32 testes Foundry** passando (16+16) · solc 0.8.22 via-ir |
| `svm/programs/relayer-reward-vault` + `svm/programs/igp-oracle-governor` (Solana) | ✅ escritos · **15 testes** solana-program-test (10+5, com mock do IGP fiel ao wire-format) · clippy limpo · **cargo build-sbf ok** (.so gerados) |
| `oracle-agent/` (off-chain, multi-chain) | ✅ escrito · 5 testes node:test · dry-run validado com CoinGecko + RPCs reais (4 chains) |

Build de produção: usar o `cosmwasm/optimizer` (build reproduzível) antes do store na chain.


## Build reproduzível (CosmWasm) — artefatos p/ a proposta

Gerado com `cosmwasm/optimizer:0.17.0` sobre o commit `27dab3b` (locks versionados —
qualquer um reproduz com `docker run ... cosmwasm/optimizer:0.17.0` e confere):

| Artefato | sha256 |
|---|---|
| `artifacts/relayer_reward_vault.wasm` (231 KB) | `c9699711a661607bebe30819ee1dc0035ff5276523dbb08b80a108fb03721d82` |
| `artifacts/oracle_governor.wasm` (268 KB) | `3383e2bc929f0d9907a95567c35ec17f4399dedc5f712b4198c244d039c41744` |

Ao armazenar na chain, o `data_hash` do code DEVE ser igual ao sha256 acima.

## Implantação — Terra Classic (Fases 1–2): ✅ NO AR (18/08/2026, columbus-5)

| Peça | Valor |
|---|---|
| **oracle-governor** | `terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj` (code_id **11587**) |
| **relayer-reward-vault** | `terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q` (code_id **11588**) |
| data_hash on-chain | ✅ = `checksums.txt` (verificado pelo script no store) |
| Owner do StorageGasOracle | ✅ = oracle-governor (posse em 2 passos, txs `31B0DF7E…`/`EDE72113…`) |
| Faixas (dom 1 · 56 · 1399811149) | ✅ derivadas do oracle de produção no momento do deploy (÷3·×3) |
| IGP.beneficiary | ✅ = vault (tx `4895068D…`; confirmado por query `{"igp":{"beneficiary":{}}}`) |
| Pool | ✅ 5.000 LUNC · tarifa 50 LUNC · **claims_payable = 100** · janela 200k blocos |
| `layout_check` (msg real `d039daa1…`) | ✅ `ok:true` — prova por raw query operando em produção |
| Operadores / quórum | 1 (deployer) / 1 — expandir via `docs/OPERADORES.md` |
| Owner (governor + vault) | deployer — handoff p/ governança: §8 de `docs/PARAMETROS_PROPOSTA.md` |

Txs principais: store `657F893F…`/`2DE362BA…` · instantiate `31DB39EB…`/`6653EFCB…` · seed `B55FD50B…`.
