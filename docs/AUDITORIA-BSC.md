# Registro de Auditoria — Deploy BSC (Fase 3)

**Data:** 2026-08-18 · **Chain:** BSC mainnet (56) · **Signer/owner:** `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291`
**Fonte:** `evm/src/*.sol` deste repositório · solc 0.8.22 via-ir · RPC `bsc-dataseed.bnbchain.org`

## Contratos implantados

| Contrato | Endereço |
|---|---|
| **RelayerRewardVault** | `0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D` |
| **GasOracleGovernor** | `0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5` |

Constructor do vault: `mailbox=0x2971b9Ae…e8a4 · owner=0x8f08…5291 ·
reward=50000000000000 (0,00005 BNB) · janela=1600000 blocos`.
Constructor do governor: `oracle=0x7dE950f8…2306 · owner=0x8f08…5291 ·
operators=[0x8f08…5291] · quorum=1 · época=21600s · delta=2000 bps`.

## Warp/IGP alvo (produção — ver WARP-IGORFAKE.md)

| Peça | Endereço |
|---|---|
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |

## Estado verificado on-chain (18/08/2026)

| Verificação | Resultado |
|---|---|
| `oracle.owner()` | ✅ = governor `0x5CF7A3a7…13E5` (posse transferida) |
| `igp.beneficiary()` | ✅ = vault `0x8b3A9eEB…145D` |
| `governor.oracle()` | ✅ = `0x7dE950f8…2306` |
| `governor.isOperator(0x8f08…5291)` | ✅ true |
| `governor.quorum()` | 1 |
| `governor.currentEpoch()` | 82735 (6h por época) |
| `setBounds(132556)` | ✅ rate [3015730·27141570] · gas [3333333333·30000000000] · vigente lido do oracle no deploy (9047190 · 1e10) ÷3·×3 |
| `vault.owner()` | `0x8f08…5291` (deployer — handoff p/ multisig na §8) |
| Pool (vault balance) | **0** — semente PULADA por saldo baixo. Semear: `cast send --legacy 0x8b3A9eEB…145D --value 5000000000000000 --private-key <PK> --rpc-url https://bsc-dataseed.bnbchain.org` |

## Pendências

- [ ] Semear o pool (0,005 BNB) quando houver saldo — sem isso `claim` reverte por `InsufficientPool`.
- [ ] Handoff: `vault`/`governor`/`igp`/`oracle`/`ISM` → multisig dos validadores (§8 de PARAMETROS_PROPOSTA.md).

## Como auditar

```bash
RPC=https://bsc-dataseed.bnbchain.org
cast call --rpc-url $RPC 0x7dE950f8F0a037783989a6BE84B3620916552306 "owner()(address)"        # = governor
cast call --rpc-url $RPC 0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923 "beneficiary()(address)"  # = vault
cast call --rpc-url $RPC 0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5 "bounds(uint32)(uint128,uint128,uint128,uint128,bool)" 132556
cast code --rpc-url $RPC 0x8b3A9eEBE949D8ce6Be651C75a54872cd382145D   # bytecode do vault (comparar com forge build)
```
