---
name: tc-pod-deploy
description: >-
  Runbook de DEPLOY e OPERAÇÃO do tc-proof-of-delivery nas 4 redes (Terra Classic,
  BSC, Ethereum, Solana): ordem das fases, comandos, transferências de posse
  (beneficiary/owner), parâmetros da proposta e monitoramento. Use quando o pedido
  for implantar, configurar governança/multisig, ou operar o sistema em produção.
---

# tc-proof-of-delivery — deploy e operação (runbook)

> Passo a passo completo: `docs/INSTALACAO_E_EXECUCAO.md` §4–§6 · diagramas do processo: `docs/ARQUITETURA.md`.
> A ordem das fases é LEI (spec §13): 0 → 1 (oracle TC) → 2 (vault TC) → 3 (EVM) → 4 (Solana).

## Fase 0 — gates antes de QUALQUER deploy
- [x] Raw query `DELIVERIES` validada em mainnet (README: 2 entregas decodificadas,
      Mailbox `terra1fwg35n...jpx3p9`, code_id 11371, relayer terra1run9wz…26mawp)
- [x] `data_hash` de TODOS os 12 contratos TC == wasms staged do tc-cw-hyperlane (README)
- [ ] Build reproduzível dos NOSSOS contratos: `cosmwasm/optimizer` (CW) · `cargo build-sbf` (SOL)
- [ ] 91 testes verdes + clippy limpo nos 2 workspaces + forge

## Sequência por rede (resumo dos pontos que quebram se inverter)
**TC:** governor → posse do oracle em 2 PASSOS (gov `init_ownership_transfer` no
oracle → `claim_oracle_ownership` no governor) → `set_bounds` POR domínio →
vault → gov aponta `IGP.set_beneficiary = vault` → semear pool → monitorar `layout_check`.

**EVM:** Vault (owner=multisig) → Governor + `setBounds` → `StorageGasOracle.transferOwnership(governor)`
(OZ, passo ÚNICO — conferir endereço 3×) → `IGP.setBeneficiary(vault)`. Sem Sweep:
o `claim()` do IGP é permissionless e o vault tem `receive()`.

**Solana:** deploy rrv + governor → Init dos dois → `SetDomainConfig` (faixa + token_decimals,
escala 1e19!) → **TESTAR `TransferIgpOwnership` EM DEVNET** → transferir posse do
IGP à config PDA do governor → **upgrade authority dos 2 programas → multisig**
(senão tudo é contornável por redeploy) → manter lamports na config PDA (realloc do IGP).

## Papéis (matriz §11 da spec — resumo)
- **Governança TC**: tudo dentro do TC (IGP, ISM, vault, oracle, tarifa, faixa).
- **Multisig** (remotas): IGP, ISM, faixa, Vault/Governor. Modelo APROVADO pela
  governança: 3 validadores do TC + 1 não-validador (4 membros). Threshold em
  aberto: 3-de-4 permite os validadores agirem sozinhos (mitigação PARCIAL do
  risco nº1 — ISM remoto = acesso indireto ao colateral); evolução: +1
  não-validador → 4-de-5. Owner fica no deployer até o fim da implantação.
- **Operadores**: preço dentro da faixa (quórum), relatórios de época (SOL),
  parâmetros do vault remoto por proposta.
- **Qualquer um**: entregar mensagens e sacar a PRÓPRIA recompensa.

## Parâmetros a fechar NA PROPOSTA (pendências §14)
tarifa/rede · janela de resgate · faixas por domínio (recalcular por VM!) ·
operadores + quórum · multisig (composição/threshold) · ISM 3-de-4 · timelock de ISM ·
decisão aberta: taxa no Warp Route como financiamento alternativo.

## Monitoramento mínimo em produção
`LayoutCheck` (TC, pós-migrate) · `Solvency`/`claimsPayable` vs backlog ·
épocas Solana sem quórum (hashes divergentes = alarme + auditoria pública) ·
preço não aplicado por `DeltaExceeded` → avaliar `ForceSet` pela governança/multisig.
