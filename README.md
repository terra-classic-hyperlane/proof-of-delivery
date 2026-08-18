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

Pendente da Fase 0 (mainnet): raw query do `DELIVERIES` numa mensagem real; conferir se o wasm
EM PRODUÇÃO bate com `~/tc-cw-hyperlane` (data_hash); hooks do Mailbox.

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
