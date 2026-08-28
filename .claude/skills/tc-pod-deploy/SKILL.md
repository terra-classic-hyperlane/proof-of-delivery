---
name: tc-pod-deploy
description: >-
  DEPLOY and OPERATION runbook for tc-proof-of-delivery across the 4 networks (Terra
  Classic, BSC, Ethereum, Solana): phase order, commands, ownership transfers
  (beneficiary/owner), proposal parameters and monitoring. Use when the request is to
  deploy, configure governance/multisig, or operate the system in production.
---

# tc-proof-of-delivery — deploy and operation (runbook)

> Full step-by-step: `docs/INSTALL-AND-RUN.md` §4–§6 · process diagrams: `docs/ARCHITECTURE.md`.
> The phase order is LAW (spec §13): 0 ✅ → 1–2 ✅ LIVE (TC, 18/08/2026 — addresses in README:
> governor terra1z7jmlky…9sv4hj / vault terra1gqkrh2…duzc2q) → 3 (EVM: BSC ✅ + ETH ✅) → 4 (Solana ✅ ACTIVE — pod 2mQZcHYL…).

## Phase 0 — gates before ANY deploy
- [x] `DELIVERIES` raw query validated on mainnet (README: 2 decoded deliveries,
      Mailbox `terra1fwg35n...jpx3p9`, code_id 11371, relayer terra1run9wz…26mawp)
- [x] `data_hash` of ALL 12 TC contracts == staged wasms of tc-cw-hyperlane (README)
- [x] Reproducible build of OUR contracts: optimizer 0.17.0 (checksums in README) · build-sbf ok
- [x] 91 tests green + clippy clean on the 2 workspaces + forge

## Sequence per network (summary of the points that break if inverted)
**TC:** governor → oracle ownership in 2 STEPS (gov `init_ownership_transfer` on the
oracle → `claim_oracle_ownership` on the governor) → `set_bounds` PER domain →
vault → gov points `IGP.set_beneficiary = vault` → seed the pool → monitor `layout_check`.

**EVM:** Vault (owner=multisig) → Governor + `setBounds` → `StorageGasOracle.transferOwnership(governor)`
(OZ, SINGLE step — check the address 3×) → `IGP.setBeneficiary(vault)`. No Sweep:
the IGP `claim()` is permissionless and the vault has `receive()`.

**Solana:** deploy the SINGLE `pod.so` (vault+governor merged, 1st byte routes
0=rrv/1=gov — rent 1.29 SOL, a single upgrade authority) → Init the 2 modules → `SetDomainConfig` (bounds + token_decimals,
scale 1e19!) → **TEST `TransferIgpOwnership` ON DEVNET** → transfer IGP ownership
to the governor's config PDA → **upgrade authority of the 2 programs → multisig**
(otherwise everything can be bypassed via redeploy) → keep lamports on the config PDA (IGP realloc).

## Roles (spec §11 matrix — summary)
- **TC governance**: everything inside TC (IGP, ISM, vault, oracle, fee, bounds).
- **Multisig** (remote networks): IGP, ISM, bounds, Vault/Governor. Model APPROVED by
  governance: 3 TC validators + 1 non-validator (4 members). Threshold still
  open: 3-of-4 lets the validators act on their own (PARTIAL mitigation of
  risk #1 — remote ISM = indirect access to the collateral); evolution: +1
  non-validator → 4-of-5. Owner stays with the deployer until deployment ends.
- **Operators**: price within bounds (quorum), epoch reports (SOL),
  remote vault parameters via proposal.
- **Anyone**: deliver messages and withdraw their OWN reward.

## Parameters to finalize IN THE PROPOSAL (open items §14)
fee/network · redemption window · bounds per domain (recompute per VM!) ·
operators + quorum · multisig (composition/threshold) · ISM 3-of-4 · ISM timelock ·
open decision: Warp Route fee as alternative funding.

## Minimum production monitoring
`LayoutCheck` (TC, post-migrate) · `Solvency`/`claimsPayable` vs backlog ·
Solana epochs without quorum (divergent hashes = alarm + public audit) ·
price not applied due to `DeltaExceeded` → evaluate `ForceSet` by governance/multisig.
