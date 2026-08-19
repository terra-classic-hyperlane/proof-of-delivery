# Registro de Auditoria — Deploy Ethereum (Fase 3)

**Data:** 2026-08-18 · **Chain:** Ethereum mainnet (1) · **Signer/owner:** `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae`
**Fonte:** `evm/src/*.sol` deste repositório · solc 0.8.22 via-ir · gás EIP-1559 (~0,09 gwei no deploy)

## Contratos implantados

| Contrato | Endereço |
|---|---|
| **RelayerRewardVault** | `0xDf90d3b7FF98466E148B334128374807b3e89EbD` |
| **GasOracleGovernor** | `0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA` |

Constructor do vault: `mailbox=0xc005dc82…D239 · owner=0xEF81…00ae ·
reward=400000000000000 (0,0004 ETH) · janela=100800 blocos`.
Constructor do governor: `oracle=0x3987cCE8…96cE · owner=0xEF81…00ae ·
operators=[0xEF81…00ae] · quorum=1 · época=21600s · delta=2000 bps`.

## Warp/IGP alvo (produção — ver WARP-IGORFAKE.md)

| Peça | Endereço |
|---|---|
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (TerraClassicIGPStandalone) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle (TerraClassicOracle) | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |

## Estado verificado on-chain (18/08/2026)

| Verificação | Resultado |
|---|---|
| `oracle.owner()` | ✅ = governor `0xa1803b36…fEFA` |
| `igp.beneficiary()` | ✅ = vault `0xDf90d3b7…9EbD` |
| `governor.isOperator(0xEF81…00ae)` / `quorum()` | ✅ true / 1 |
| `setBounds(132556)` | ✅ rate [8861692·79755234] · gas [3333333333·30000000000] · vigente lido do oracle no deploy (26585078·1e10) ÷3·×3 |
| `governor.currentEpoch()` | 82735 |
| Pool (vault balance) | **0** — semente PULADA por saldo baixo. Semear: `cast send 0xDf90d3b7…9EbD --value 40000000000000000 --private-key <PK> --rpc-url https://ethereum-rpc.publicnode.com` (pode reduzir o valor) |

## Pendências

- [ ] Semear o pool (sugestão 0,04 ETH; qualquer valor serve para começar).
- [ ] Handoff: vault/governor/igp/oracle/ISM → multisig dos validadores (§8).

## Vault v2 (ClaimRemote) — 19/08/2026

Deploy novo `0x04096dCBbBB0FA58a312761c38E1d3B9F64631F1` (v1 `0xDf90d3b7…9EbD`
deprecado, pool 0). `igp.setBeneficiary(v2)` ✓ · atestador `0xEF81…00ae` quórum 1 ·
vínculo dom 132556 → `terra1run9wz…` · recompensa remota **9.294.377.050.000 wei**
(= taxa real: (50k+300k overhead) × gasPrice 1e10 × rate 26555363 / 1e10).
