# tc-proof-of-delivery — Instalação e Execução

Documentação técnica dos 7 artefatos da spec (`SPEC.html`): como preparar o
ambiente, compilar, testar, e implantar cada camada. As decisões de arquitetura
estão na spec; a verificação de mainnet (Fase 0 parcial) está no `README.md`.

---

## 1. Layout do repositório

```
tc-proof-of-delivery/
├── SPEC.html                     # especificação v3 (fonte da verdade do desenho)
├── README.md                     # verificação de código/mainnet + placar de testes
├── Cargo.toml                    # workspace COSMWASM
├── contracts/
│   ├── relayer-reward-vault/     # vault TC: prova por raw query no Mailbox
│   └── oracle-governor/          # quórum+mediana+faixa sobre o StorageGasOracle
├── evm/                          # projeto FOUNDRY (BSC/Ethereum)
│   ├── src/RelayerRewardVault.sol
│   ├── src/GasOracleGovernor.sol
│   └── test/
├── svm/                          # workspace SOLANA
│   └── programs/
│       ├── relayer-reward-vault/ # crate "rrv": pool na PDA, épocas, propostas
│       ├── igp-oracle-governor/  # duas portas sobre o IGP
│       └── mock-igp/             # SÓ TESTES: espelho do wire-format do IGP real
└── oracle-agent/                 # Node: feed de preço multi-chain p/ os governors
```

Cada camada tem toolchain e workspace PRÓPRIOS — não misture (o CosmWasm e o
Solana têm árvores de dependências incompatíveis entre si).

---

## 2. Pré-requisitos

| Ferramenta | Versão testada | Uso |
|---|---|---|
| Rust + cargo | 1.84.0 | CosmWasm e Solana (⚠️ ver nota de pins abaixo) |
| target `wasm32-unknown-unknown` | — | build CosmWasm (`rustup target add wasm32-unknown-unknown`) |
| Foundry (forge) | 1.5.0 | EVM (build + testes) |
| Solana CLI + cargo-build-sbf | 4.0.0 / platform-tools 1.53 | build BPF + deploy Solana |
| Node.js | 20.x | oracle-agent |
| Docker | qualquer | build reproduzível CosmWasm (`cosmwasm/optimizer`) |

> **Nota sobre os `Cargo.lock`:** com rustc 1.84, várias dependências
> transitivas recentes exigem `edition2024` (rustc ≥1.85). Os DOIS lockfiles
> (`Cargo.lock` da raiz e `svm/Cargo.lock`) já estão fixados em versões
> compatíveis — **commite-os e não rode `cargo update` sem necessidade**. Se
> precisar atualizar algo, atualize pontualmente
> (`cargo update <crate>@<ver> --precise <ver-compatível>`).

---

## 3. Build e testes por camada

### 3.1 CosmWasm (Terra Classic)

```bash
cd tc-proof-of-delivery
cargo test                                     # 39 testes (unit + cw-multi-test)
cargo clippy --all-targets -- -D warnings      # limpo
cargo build --release --target wasm32-unknown-unknown --lib   # wasm de desenvolvimento
```

**Build de PRODUÇÃO (reproduzível — obrigatório antes do store na chain):**

```bash
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.0
# artefatos em ./artifacts/*.wasm + checksums.txt (é o data_hash do code na chain)
```

### 3.2 EVM (BSC / Ethereum)

```bash
cd evm
# 1ª vez após o clone (forge-std não é versionado):
git clone --depth 1 https://github.com/foundry-rs/forge-std lib/forge-std
forge test            # 32 testes
forge build --sizes   # Vault ~2,5 KB · Governor ~6,5 KB (via_ir habilitado)
```

### 3.3 Solana

```bash
cd svm
cargo test            # 15 testes funcionais (solana-program-test, execução nativa)
cargo clippy --all-targets -- -D warnings
cargo build-sbf       # gera target/deploy/{rrv,igp_oracle_governor}.so
```

O `mock-igp` é artefato de teste — **nunca** vai para mainnet.

### 3.4 oracle-agent

```bash
cd oracle-agent
npm install
npm test              # matemática do exchange_rate/escalas
npm run dry-run       # rodada completa SEM assinar (CoinGecko + RPCs reais)
```

---

## 4. Implantação (ordem da spec §13 — resumo executável)

> **FASE 0 (obrigatória antes de tudo):** parcialmente feita — a raw query do
> `DELIVERIES` foi validada NA MAINNET (ver README). Falta comparar o
> `data_hash` do code_id 11371 com o `checksums.txt` do build reproduzível do
> `tc-cw-hyperlane` em produção.

### 4.1 Terra Classic (Fases 1–2)

```bash
# 1. store + instantiate do oracle-governor
terrad tx wasm store artifacts/oracle_governor.wasm --from operador ...
terrad tx wasm instantiate <code_id> '{
  "owner": "<ENDEREÇO_DO_MÓDULO_GOV>",
  "oracle": "<hpl-igp-oracle>",
  "operators": ["terra1...","terra1..."],
  "quorum": 2,
  "epoch_duration_secs": 21600,
  "max_delta_bps": 2000
}' --label "oracle-governor" ...

# 2. posse do oracle → governor (2 passos):
#    a) governança executa no oracle: {"ownership":{"init_ownership_transfer":{"next_owner":"<governor>"}}}
#    b) qualquer um executa no governor: {"claim_oracle_ownership":{}}

# 3. governança define a faixa POR DOMÍNIO no governor:
#    {"set_bounds":{"domain":56,"bounds":{...}}}

# 4. store + instantiate do relayer-reward-vault
terrad tx wasm instantiate <code_id> '{
  "owner": "<gov>", "mailbox": "terra1fwg35n...jpx3p9",
  "igp": "<hpl-igp>", "denom": "uluna",
  "reward_per_delivery": "<ex.: 1000000>",
  "claim_window_blocks": <ex.: 100000>
}' ...

# 5. governança: IGP {"set_beneficiary":{"beneficiary":"<vault>"}}
# 6. semear o pool (BankSend de uluna ao vault) e monitorar {"layout_check":{...}}
```

### 4.2 BSC / Ethereum (Fase 3)

```bash
cd evm
forge create src/RelayerRewardVault.sol:RelayerRewardVault \
  --constructor-args <MAILBOX> <MULTISIG> <REWARD_WEI> <WINDOW_BLOCKS> ...
forge create src/GasOracleGovernor.sol:GasOracleGovernor \
  --constructor-args <STORAGE_GAS_ORACLE> <MULTISIG> '[<op1>,<op2>]' 2 21600 2000 ...

# multisig:
#   governor.setBounds(domain, {...})
#   StorageGasOracle.transferOwnership(governor)     # OZ, passo único
#   IGP.setBeneficiary(vault)                        # claim() do IGP é permissionless
```

### 4.3 Solana (Fase 4)

```bash
cd svm && cargo build-sbf
solana program deploy target/deploy/rrv.so
solana program deploy target/deploy/igp_oracle_governor.so

# 1. Init do rrv → a PDA de config ("rrv-config") é o POOL:
#    registre-a como beneficiary do IGP (via governor: SetIgpBeneficiary)
# 2. Init do governor (multisig, operadores, quórum, época, delta, igp_program, igp)
# 3. multisig: SetDomainConfig por domínio (faixa + token_decimals — escala 1e19!)
# 4. ⚠️ TESTAR TransferIgpOwnership EM DEVNET antes do passo 5 (spec §08)
# 5. IGP real: TransferIgpOwnership(config_pda_do_governor)
# 6. ⚠️ upgrade authority dos DOIS programas → multisig:
solana program set-upgrade-authority <PROGRAM_ID> --new-upgrade-authority <MULTISIG>
# 7. manter lamports na PDA de config do governor (o realloc do IGP cobra do owner)
```

### 4.4 oracle-agent (todas as chains)

```bash
cd oracle-agent && cp config.example.json config.json
# preencher: governors, RPCs, domínios (TC = 132556), fontes de gás
TC_MNEMONIC=... EVM_PRIVATE_KEY=... SOLANA_KEYPAIR_PATH=... npm run once   # cron 6h/6h
```

Cada operador roda o SEU agente com a SUA chave — sem coordenação; o governor
converge pela mediana.

---

## 5. Operação e monitoramento

| O quê | Como | Alarme |
|---|---|---|
| Layout do Mailbox TC | query `{"layout_check":{"message_id":"<id entregue>"}}` no vault | `ok:false` com "VALUE LAYOUT MISMATCH" → migrate mudou o layout; PAUSAR claims |
| Solvência do pool | query `{"solvency":{}}` (TC) · `claimsPayable()` (EVM) | capacidade < backlog de entregas |
| Época Solana travada | 2+ hashes divergentes na `EpochState` | auditoria manual dos relatórios vs chain pública |
| Preço não aplicado | `Applied{domain,epoch}` vazio após a época | quórum não convergiu ou `DeltaExceeded` → avaliar `ForceSet` |

## 6. Parâmetros a definir NA PROPOSTA (spec §14)

Tarifa por entrega em cada rede · janela de resgate · faixas do oracle por
domínio/rede · endereços dos operadores + quórum · composição/threshold do
multisig (com signatários que NÃO sejam validadores Hyperlane) · threshold do
ISM (3-de-4) · timelock para troca de ISM.
