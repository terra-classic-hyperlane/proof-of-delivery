# Installation Report — oracle-agent (audit)

**Date:** 2026-08-18 · **Server:** Hyperlane relayer VPS (31.97.91.4, Ubuntu 24.04.4)
**Installed by:** Claude Code, at the operator's request (igor.veras@gmail.com)

## What was installed

| Item | Value |
|---|---|
| Runtime | Node v22.14.0 (official tarball → /usr/local) · npm 10.9.2 |
| Code | `/root/oracle-agent` (rsync of this repo, install commit) · deps `npm install --omit=dev` |
| Config | `/root/oracle-agent/config.json` — 4 chains enabled · **interval 3600 s (1 h)** · `minChangeBps` 300 |
| Service | `/etc/systemd/system/oracle-agent.service` (Restart=always, RestartSec=60) |
| Logs | `/root/oracle-agent/logs/agent.log` (append; + journald) |
| Keys | `.env` created by `setup-env.sh` **run by the operator** (relayer hex; never left the server; chmod 600) |

## Code changes made for this installation (commit of this date)

1. **Universal HEX key** — the 3 modules accept the relayer hex key:
   TC `DirectSecp256k1Wallet.fromKey` · EVM `Wallet(hex)` · Solana `Keypair.fromSeed(hex)`.
2. **ANCHOR mode** — the agent reads the CURRENT value of each oracle and only adjusts it
   by the relative variation (does not compute from scratch). Reason: the pre-installation
   dry-run revealed that the canonical formula diverged from the production calibration (e.g.: BSC
   would compute 789 vs 9047190 current — every submission would be rejected by the
   bounds or, with wide bounds, would break the warp fees).
3. `readOracle()` per chain (CosmWasm query / EVM call / Igp account scan).

## Validation (dry-run ON THE SERVER, 08/18 20:07 UTC — signing nothing)

```
[agent] oracle-agent starting · chains: terraclassic, bsc, ethereum, solana · DRY-RUN
[agent] USD prices: {"terraclassic":0.00004749,"ethereum":1911.19,"bsc":602.37,"solana":77.01}
[ethereum]     domain 132556: anchor would be created at current rate=26585078     gas=10000000000
[terraclassic] domain 1:      anchor would be created at current rate=376          gas=10000000000
[solana]       domain 132556: anchor would be created at current rate=29400000000  gas=28325
[terraclassic] domain 56:     anchor would be created at current rate=1098         gas=3000000000
[terraclassic] domain 1399811149: anchor would be created at current rate=383001553014 gas=1
[bsc]          domain 132556: anchor would be created at current rate=9047190      gas=10000000000
```

✅ The 6 routes read EXACTLY the production values documented in
`WARP-IGORFAKE.md` — on-chain reading validated on the 4 networks before activation.

## Operators registered in the governors (state at installation)

| Chain | Operator (= `.env` key) | Registered? |
|---|---|---|
| Terra Classic | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` | ✅ (deploy Phase 1-2) |
| BSC | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` | ✅ (deploy Phase 3) |
| Ethereum | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` | ✅ (deploy Phase 3) |
| Solana | `PbEo7Fn2eJ6LYa4B8YU4MexB6s1BEQquWKCM1cwwrkS` | ✅ tx `284meW1GYxVrjc4KGbrFU5kBccL4hVpda1DVwZRmLZLHZ1xmuDL2NWGxrdyLvAt25AhbL7ExC2LSmpzcNhfQ3xdW` (verified in the gov config) |

## Activation (08/18 20:12 UTC, explicitly authorized by the operator)

`.env` created (chmod 600) · Solana operator registered · `systemctl enable --now
oracle-agent` → service **active**. First REAL round (production log):

```
[agent] oracle-agent starting · chains: terraclassic, bsc, ethereum, solana
[ethereum]     132556: anchor created at current rate=26585078     — nothing submitted
[bsc]          132556: anchor created at current rate=9047190      — nothing submitted
[terraclassic] 1:      anchor created at current rate=376          — nothing submitted
[terraclassic] 56:     anchor created at current rate=1098         — nothing submitted
[terraclassic] 1399811149: anchor created at current rate=383001553014 — nothing submitted
[solana]       132556: anchor created at current rate=29400000000  — nothing submitted
[agent] next round in 3600s (loop)
```

✅ The 6 anchors created exactly at the production values; no submission on the
debut (designed behavior). From the next round on (1 h), submits
only if the drift ≥ 3%, always within the on-chain bounds/delta/quorum.

## End-to-end test (08/18 20:49–20:53 UTC — REAL submissions)

With a temporary `minChangeBps=0`, one real round submitted on the 6 routes:

| Route | Tx | Result |
|---|---|---|
| TC → dom 1 (ETH) | `CA15CE3C0ED2B29D1F3028E3ABB9EEF214D2BD25C559E463D8A9B800EC0CBA92` | ✅ applied |
| TC → dom 56 (BSC) | `044EE80E81B049D95C08E67B1508E63E46220810FC36631EB586292E7C627D28` | ✅ applied |
| TC → dom SOL | `1DCA67D5B6C2DDB383BEF9EECA6F02383B3D0C7140356D4E524901D16B90D8D2` | ✅ applied |
| BSC → dom 132556 | `0x87cb7dc1af9066c35d710c52a8d7d866dc4bcd0eb9eef4e32467d2db663e3300` | ✅ applied |
| ETH → dom 132556 | `0x453dc7213306e940cb63f0e10111cb70a1009d6c960e48f144bfe1285bce5ce3` | ✅ applied |
| SOL → dom 132556 | `wbYQMDyobCgkoReWph9SPQMcqUoLTxYAMhJqoWovxs5vkMtRV7LhJh37o4AUg5PRZjhLwqiSqu7n2AtkBm12Ttk` | ✅ applied (IGP: 29400000000 → 29484263762, verified) |

Two defects found and fixed by the test:
1. **config without `privateKeyEnv` on Solana** → 1st attempt failed with `env
   SOLANA_KEYPAIR_PATH missing`; fixed in config.json and in the example.
2. **Quorum 2 in the Solana governor** (the init ran with OPERATOR2 in the environment) →
   the submission was recorded but never applied (CPI absent in the tx logs
   `eEH96Mtq…`). Adjusted to quorum 1 via multisig (tx `bbpnAfwZ…`); retest
   applied on the real IGP. `minChangeBps` restored to 300; service active.

## Addendum (08/18 22:27 UTC): claim-agent integrated and active

- `src/claims.js` added to the SAME service (phase 2 of each 1 h round):
  automatic TC/EVM claim + epoch report/withdraw on Solana.
- TC scanner validated against the REAL delivery `d039daa1…a28c4f04` (block
  29422362, relayer terra1run9wz…): id and sender extracted correctly.
  That specific delivery is EXPIRED for claim (586,753 blocks > 200,000
  window) — the automatic redemption applies to new deliveries.
- **Solana vault quorum reduced 2→1** by the proposal flow §09 with the TWO
  approvals (BirXd4 `VRQUgUzx…`, PbEo `f2DPjZdB…` — executes on the 2nd): without this,
  epoch reports would never credit with 1 active operator. As a bonus, the
  multi-operator admin flow was validated in production.
- Initial cursors recorded on the 4 chains (TC 30009127 · BSC 116736155 ·
  ETH 25784960 · SOL 3LRV8CuM…). Service `active`, next rounds every 1 h.
