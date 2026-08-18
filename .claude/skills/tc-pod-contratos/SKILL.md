---
name: tc-pod-contratos
description: >-
  Runbook de DESENVOLVIMENTO dos contratos do tc-proof-of-delivery (remuneração de
  relayers Hyperlane em 4 redes): vault + oracle-governor CosmWasm, Vault/Governor
  Solidity, programas Solana (rrv + igp-oracle-governor) e seus testes. Use ao
  alterar, revisar, testar ou estender qualquer contrato/programa deste repo.
---

# tc-proof-of-delivery — contratos (runbook de desenvolvimento)

> Fonte da verdade do desenho: `SPEC.html` (v3) · diagramas: `docs/ARQUITETURA.md`. Evidências de mainnet e placar
> de testes: `README.md`. Build/deploy: `docs/INSTALACAO_E_EXECUCAO.md`.

## Princípio inegociável
O operador recebe pelo que **ENTREGOU**, provado pelo registro da própria chain:
- **TC**: raw query no storage do Mailbox — chave `[0x00,0x0A]+"deliveries"+message_id`,
  valor JSON `{"sender","block_number"}` (CONFIRMADO em produção, code_id 11371);
- **EVM**: `mailbox.processor(id)` / `processedAt(id)` (Mailbox v3);
- **Solana**: a chain NÃO grava o executor → quórum de operadores por época.
Nenhuma linha do core Hyperlane é modificada — só configuração (beneficiary/owner).

## Mapa de código
| Camada | Onde | Testar |
|---|---|---|
| Vault TC | `contracts/relayer-reward-vault/` | `cargo test` (24) |
| OracleGovernor TC | `contracts/oracle-governor/` | `cargo test` (15) |
| EVM | `evm/src/*.sol` + `evm/test/` | `cd evm && forge test` (32) |
| Solana | `svm/programs/{relayer-reward-vault,igp-oracle-governor}` | `cd svm && cargo test` (15) |
| mock IGP (SÓ teste) | `svm/programs/mock-igp` | espelha índices borsh 5/7/9 + contas do IGP real |

## Invariantes que os testes protegem (não quebrar)
1. **Claim atômico**: um id inválido reverte o lote; nada é consumido.
2. **Effects-first**: registro de pagamento gravado ANTES do transfer.
3. **Parse ESTRITO no TC** (`deny_unknown_fields`): migrate do Mailbox → erro
   `MailboxLayoutMismatch`, nunca pagamento errado. Monitorar via query `LayoutCheck`.
4. **Mediana = menor dos centrais** no empate par (cobra menos do usuário).
5. **Faixa é da governança/multisig, nunca dos operadores** (conflito de interesse).
6. **Delta máximo (bps)** vs último aplicado; estourou → só emergência resolve.
7. Solana §09: janela de slots **trava na 1ª submissão**; lista de créditos
   **estritamente ordenada**; destino do `WithdrawSurplus` **dentro do hash** do envelope.
8. `Sweep` (TC) permissionless; EVM não precisa (claim do IGP é permissionless + `receive()`).
9. Escalas: exchange_rate 1e10 (CW/EVM) vs **1e19 (Solana)** — nunca copiar faixas entre VMs.

## Interfaces externas (NÃO inventar — foram conferidas nos repos reais)
- IGP CW (`~/tc-cw-hyperlane`): `{"claim":{}}` só beneficiary; oracle ownership em
  2 passos `InitOwnershipTransfer`→`ClaimOwnership`; `SetRemoteGasData{config}`.
- EVM (`~/hyperlane-monorepo/solidity`): `StorageGasOracle` é OZ Ownable passo único.
- Solana (`~/hyperlane-monorepo/rust/sealevel`): instrução borsh do IGP —
  Transfer=5, SetBeneficiary=7, SetGasOracleConfigs=9; contas 9=[system, igp w,
  owner signer], 5/7=[igp w, owner signer]; `RemoteGasData` tem `token_decimals`.

## Toolchain / armadilhas
- rustc 1.84: os `Cargo.lock` têm ~20 pins anti-edition2024 — **não** rodar
  `cargo update` amplo; atualizar com `--precise` pontual.
- EVM compila com `via_ir = true` (stack too deep no submitPrice sem isso).
- Solana: lints `unexpected_cfgs` liberados no workspace (falso positivo do
  entrypoint! 1.18); `cargo build-sbf` gera os `.so`.
- Antes de qualquer PR: `cargo test` + `clippy -D warnings` (2 workspaces) +
  `forge test` — 91 testes no total, tudo verde.
