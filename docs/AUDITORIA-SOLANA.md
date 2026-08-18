# Registro de Auditoria — Deploy Solana (Fase 4)

**Data:** 2026-08-18 · **Chain:** Solana mainnet (1399811149) · **Signer:** `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (owner do IGP)
**Binário:** `pod.so` 184.904 bytes (vault **rrv** + **igp-oracle-governor** FUNDIDOS —
1º byte do instruction data roteia: `0x00`=vault · `0x01`=governor) · deploy com
`--max-len` exato · **custo real: 1,359 SOL** (rent recuperável via `program close`).

## Programa e PDAs

| Peça | Endereço |
|---|---|
| **pod program (vault+governor)** | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` |
| rrv config PDA (**o POOL** / futuro beneficiary) | `Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w` |
| gov config PDA (futuro owner do IGP) | `4sZAfqDqEmR7LMWjrdNmoEkv8S6BDdnDkh5mfADenaaA` |
| Upgrade authority (→ multisig no handoff) | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

## IGP alvo (produção — ver WARP-IGORFAKE.md)

IGP program `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` · IGP account (inner,
recebe pagamentos) `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk`.

## Transações (mainnet)

| Passo | Assinatura |
|---|---|
| rrv Init | `3tsaB5tyXn5aBGQYTJQYkohXTD2uPMS4xecH8TuQfbeqrwbtyYREd2hM6RRSLGCq3GrphQqyQPAdMaJrD5D1wGE5` |
| governor Init | `5nQ1RT6cE17se8DdHKRoNqjgpKw622uQT51cBqqyEKA2mXq5pgm7y1kpuxSWuQ6zank8AafCAqbNF6QBftoadNkX` |
| SetDomainConfig(132556) | `sqaxY7DPNcruCyXwH8BRo1hNvxB6HXfGZBcsnAMnKzUw7YdyQrXumfxtvfWFr2J3eNMHbxHYot6rxpAbrAUiDiJ` |
| top-up gov config PDA (0,05 SOL) | `29GbV7LudCwjnAJNRUg7y5ocMurwvCuszcAtPRJ3Vsa7imgXirixbsMNA2JKR4yaSxXy3Jp7Z4etqaJCH8oKYv1T` |

## Parâmetros gravados

- Vault: tarifa 0,003 SOL/entrega · época 21600 s · operadores `[BirXd4Q…]` · quórum 1.
- Governor: delta 2000 bps · época 21600 s · faixas do domínio 132556:
  rate `[9800000000 · 88200000000]` · gas `[9441 · 84975]` · decimals 6.
- Faixas derivadas de rate=29400000000/gas=28325: o init caiu no fallback de
  09/07 (bug de offset no parser do account Igp), mas a leitura on-chain
  posterior (18/08) **confirmou byte a byte que o vigente é idêntico** — faixas
  corretas. Parser corrigido no mesmo dia (varredura validada, testada contra
  o mainnet: layout real = `01` initialized + `"IGP_____"` + bump + salt +
  Option<owner> + beneficiary + HashMap).

## Pendências (finalize — deliberadamente separado)

- [ ] Testar `TransferIgpOwnership` ida-e-volta em **devnet** (spec §08).
- [ ] `bash deploy/solana-deploy.sh finalize` → posse do IGP → gov config PDA ·
      beneficiary → rrv pool · semente 0,3 SOL. **Até lá o IGP segue pagando o
      beneficiary antigo e o pool está vazio.**
- [ ] Registrar o operador do relayer (`PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS`)
      no conjunto de operadores (hoje só o deployer, quórum 1).
- [ ] Upgrade authority do pod → multisig dos validadores (§8).
