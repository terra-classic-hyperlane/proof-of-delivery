# Audit Record — Solana Deploy (Phase 4)

**Date:** 2026-08-18 · **Chain:** Solana mainnet (1399811149) · **Signer:** `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (IGP owner)
**Binary:** `pod.so` 184,904 bytes (vault **rrv** + **igp-oracle-governor** MERGED —
1st byte of the instruction data routes: `0x00`=vault · `0x01`=governor) · deploy with
exact `--max-len` · **actual cost: 1.359 SOL** (rent recoverable via `program close`).

## Program and PDAs

| Piece | Address |
|---|---|
| **pod program (vault+governor)** | `2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj` |
| rrv config PDA (**the POOL** / future beneficiary) | `Eq1mJGTSbLb8s6gfoyg5aovxFAhXpnVudXXSAmbDwb9w` |
| gov config PDA (future IGP owner) | `4sZAfqDqEmR7LMWjrdNmoEkv8S6BDdnDkh5mfADenaaA` |
| Upgrade authority (→ multisig at handoff) | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

## Target IGP (production — see WARP-IGORFAKE.md)

IGP program `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` · IGP account (inner,
receives payments) `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk`.

## Transactions (mainnet)

| Step | Signature |
|---|---|
| rrv Init | `3tsaB5tyXn5aBGQYTJQYkohXTD2uPMS4xecH8TuQfbeqrwbtyYREd2hM6RRSLGCq3GrphQqyQPAdMaJrD5D1wGE5` |
| governor Init | `5nQ1RT6cE17se8DdHKRoNqjgpKw622uQT51cBqqyEKA2mXq5pgm7y1kpuxSWuQ6zank8AafCAqbNF6QBftoadNkX` |
| SetDomainConfig(132556) | `sqaxY7DPNcruCyXwH8BRo1hNvxB6HXfGZBcsnAMnKzUw7YdyQrXumfxtvfWFr2J3eNMHbxHYot6rxpAbrAUiDiJ` |
| top-up gov config PDA (0.05 SOL) | `29GbV7LudCwjnAJNRUg7y5ocMurwvCuszcAtPRJ3Vsa7imgXirixbsMNA2JKR4yaSxXy3Jp7Z4etqaJCH8oKYv1T` |

## Recorded parameters

- Vault: fee 0.003 SOL/delivery · epoch 21600 s.
- Governor: multisig `BirXd4Q…` · operators `[BirXd4Q…, PbEo7Fn2…]` ·
  **quorum 1** (init recorded 2 — the deploy ran with OPERATOR2 in the environment;
  adjusted to 1 on 08/18, tx `bbpnAfwZSVyfqivUNksy8VFhJmwZ7yManksR2FDUNxo2ztHVGPevfFm2zTpbEdB7p8CLGYqUQC8R2UYdVUkAAbQ`,
  since there is 1 active operator in this phase) · delta 2000 bps · domain 132556 bounds:
  rate `[9800000000 · 88200000000]` · gas `[9441 · 84975]` · decimals 6.
- Bounds derived from rate=29400000000/gas=28325: the init fell into the 07/09
  fallback (offset bug in the Igp account parser), but the later on-chain read
  (08/18) **confirmed byte by byte that the current value is identical** — bounds
  correct. Parser fixed the same day (validated sweep, tested against
  mainnet: real layout = `01` initialized + `"IGP_____"` + bump + salt +
  Option<owner> + beneficiary + HashMap).

## FINALIZE executed (08/18/2026) — system ACTIVE

| Step | Signature |
|---|---|
| IGP `TransferIgpOwnership` → gov config PDA | `Wt4vkvH5TfMKCWYAdecw3miW4VntP3CgM2XWJY7KfnJVwPMFia3eF9ytgdkPbRgbA3as8Gkv2hvS6GySMGAz8ax` |
| governor `SetIgpBeneficiary` → rrv pool | `3akHxRFZsi2RkeebkPbrwk5pdXeAHgJNyjux1SPCUk9VWNcGGw8ZkhPe9adKza31Pxfn9pujCqe5tHMxokpJJ5oz` |
| seed 0.3 SOL → pool | `2acuCBZkcyPfFbQDiW4NVGkhoVRWpK7qGtSqGVEVdPR4z2iAHoeMrWg7iURvzKKuGVBKmv3QPpdPXJrYuLGywNJc` |

Independent post-finalize verification (read of the Igp account in production):
**owner = gov config PDA ✓** (offset 43) · **beneficiary = rrv pool ✓** (offset 75) ·
pool with **0.308 SOL** · gov PDA with 0.058 SOL for realloc.

Process note: the devnet round-trip test (spec §08) was SKIPPED by
operator decision. Mitigation: the governor has the **emergency exit**
`TransferIgpOwnership(Option<Pubkey>)` (owner-only, tested in the suite — 15 tests),
which returns IGP ownership to any key if needed.

## Final pending items

- [ ] Register the relayer operator (`PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS`)
      in the operator set (currently only the deployer, quorum 1).
- [ ] Upgrade authority of pod → validator multisig (§8):
      `solana program set-upgrade-authority 2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj --new-upgrade-authority <MULTISIG>`
- [ ] oracle-agent: real config filled in (pod/IGP) and pod module prefix
      APPLIED in `src/chains/solana.js` (fixed 08/18 — without it SubmitPrice
      would be rejected with InvalidInstructionData).

## v2 ClaimRemote — 08/19/2026 (in-place upgrade of the pod)

- `solana program extend` +21,856 bytes and **upgrade on the SAME program id**
  (`2mQZcHYL…`, tx `5q8eG4qUKKq6ziAZaMdgkzR8vpCekLjW3m5wrg8aZmx4DJ2hVtVzZA4Lu89GnLjRmuM4aWXe1yHmYrP5E1x8hncq`).
  pod.so v2: 206,760 bytes. PDAs/state preserved.
- Solana design: instead of one PDA per message (rent > reward), remote
  credits enter the existing **epoch report** (`EpochReport.remote`),
  same hash/quorum, withdrawn via the normal `Withdraw`. 12 tests (2 new).
- Config via administrative proposal (quorum 1):
  `SetRemoteReward(132556, 499000)` tx `5GXH29C85YtbLBLoNY6qKMyQwPCBHJXrRkWFaaf8d3MJv2MY4jFNyJH7xyN822RAhLoGudENX6t7izi5CrWBRbXx`
  (499,000 lamports = REAL fee measured in dispatch `4wiG4TtZ…`) ·
  `SetRemoteBinding(132556, PbEo7Fn2… → terra1run9wz…)` tx `4J3tCCkV7T96hhaTSqmtBPBne2X7xASTCVw8XWYzknpEpBCxPvtuJTfR5dFq3iDd1yj4s49XnnUeKXoXRGF97gex`.
  PDAs: reward `8N3sq5XgZGn2xJ22hhNZXmTpv2TS4sVPAdXPp8GUVDDs` · binding `GTeqFxoQgfUJipvgjKxRZZsKGDEbMLSSuDXaVPEd3ez4`.
