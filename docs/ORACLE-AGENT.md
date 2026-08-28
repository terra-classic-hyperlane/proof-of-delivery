# oracle-agent + claim-agent — installation, execution, and auditing

The operator's off-chain agent: every **1 hour** (`intervalSeconds: 3600`) it quotes
prices (CoinGecko) and gas, and submits `SubmitPrice` to the governors of the 4 networks.
Quorum, median, bounds, and delta are enforced **on-chain** — the agent decides nothing.

## Phase 2 of each round: CLAIMS (the claim-agent)

In the same process/service, after the prices, the agent **claims the payments
for the relayer's deliveries** (`src/claims.js`, config `chains.<name>.claims`):

- **TC**: scans `wasm-mailbox_process_id` on the Mailbox (tx_search) filtering by
  `message.sender = relayer`, checks `claimed`/solvency, and calls `Claim` in a batch.
- **BSC/ETH**: scans the Mailbox `ProcessId` event (getLogs, 2000-block
  windows), filters `mailbox.processor(id) = relayer`, checks `claimedBy`/pool, and
  calls `claim(ids)`. Without a pool, the ids stay PENDING in the state (seed the vault).
- **Solana**: counts the `process()` calls paid by the relayer on the Mailbox per epoch;
  epoch closed → `SubmitEpochReport` (the pod's rrv module; on-chain quorum) →
  `Withdraw` of the available credit (respecting the pool's rent).
- **v2 ClaimRemote (ORIGIN fee)**: the TC scanner captures the ORIGIN of each
  of your deliveries and the agent attests on the origin vault — TC attests by id
  (`remoteAttest`), BSC/ETH by id (`remoteAttestEvm`), Solana by epoch
  (`remoteAttestSol` → `remote` field of the report). All in the SAME hourly
  cycle.

Cursors/pending items/epochs live in `state.json`. **The first round only records the
cursor** — only NEW deliveries are claimed automatically (old ones:
manual, `OPERACAO-CONTRATOS.md`). TC window: 200k blocks; BSC 1.6M; ETH 100.8k.

## ANCHOR mode (why it does not compute the price from scratch)

Each warp deployment has its **own calibration** (e.g.: BSC current 9047190;
the canonical formula would give 789). That is why the agent:

1. on the 1st round, **reads the CURRENT on-chain value** of each oracle and records it as
   an anchor in `state.json` (together with the USD price ratio at that moment) —
   **nothing is submitted**;
2. on subsequent rounds, it computes the candidate = anchor × (relative change of
   the USD price) and gas = anchor × (change of the observed gas);
3. it only submits if the drift vs the current value ≥ `minChangeBps` (default 300 = 3%).

Did you recalibrate the oracle manually? **Delete the corresponding entry from
`state.json`** and the agent re-anchors on the new value in the following round.

## Keys — ALL in HEX (the same ones as the Hyperlane relayer)

| Env (`.env`) | Chain | Format |
|---|---|---|
| `TC_PRIVATE_KEY` | Terra Classic | secp256k1 hex (cosmosKey) |
| `BSC_PRIVATE_KEY` / `ETH_PRIVATE_KEY` | BSC / Ethereum | secp256k1 hex |
| `SOL_PRIVATE_KEY` | Solana | ed25519 seed hex (32 bytes) |

The account of each key must be **registered as an operator** on the governor of the
respective chain (see `OPERACAO-CONTRATOS.md`) and have a minimum balance for gas.

## Production installation (relayer VPS) — state as of 2026-08-18

Already installed at `/root/oracle-agent` (Node v22.14.0, deps `--omit=dev`):

```bash
# 1. .env with the relayer keys (RUN ONCE — keys stay on the server):
ssh root@31.97.91.4 'bash /root/oracle-agent/setup-env.sh'
# 2. validate without signing anything:
ssh root@31.97.91.4 'cd /root/oracle-agent && node src/index.js --once --dry-run'
# 3. enable the service (1h loop, automatic restart):
ssh root@31.97.91.4 'systemctl enable --now oracle-agent && systemctl status oracle-agent --no-pager'
```

Production config: `/root/oracle-agent/config.json` (4 chains enabled,
real governors/oracles, 3600 s interval). Unit: `/etc/systemd/system/oracle-agent.service`.

## Logs and auditing

- **Continuous (auditable) log:** `/root/oracle-agent/logs/agent.log` — each line
  has an ISO timestamp, chain, domain, current value, candidate, drift, and the **tx hash**
  of each submission. Also in the journal: `journalctl -u oracle-agent`.
- **Anchors:** `/root/oracle-agent/state.json` (value + ratio + timestamp of
  each anchor — evidence of which calibration the agent is preserving).
- **On-chain trail (independent of the log):** `submissions`/`applied` queries
  on the TC governor, `PriceSubmitted/PriceApplied` events on the EVM governors,
  and the operator wallet's `SubmitPrice` txs on each explorer.

```bash
ssh root@31.97.91.4 'tail -50 /root/oracle-agent/logs/agent.log'   # latest rounds
ssh root@31.97.91.4 'cat /root/oracle-agent/state.json'            # current anchors
```

## Useful commands

```bash
node src/index.js --once --dry-run   # simulates one round, signs nothing
node src/index.js --once             # one real round (manual cron)
npm test                             # unit tests
systemctl restart oracle-agent       # after editing config.json
```

## Troubleshooting

| Symptom in the log | Likely cause | Action |
|---|---|---|
| `anchor created … nothing submitted` | 1st round of that domain | Normal |
| `stable (<300bps)` | price did not move enough | Normal |
| `NotOperator` / `Unauthorized` | key not registered as an operator | `OPERACAO-CONTRATOS.md` (SetOperators) |
| `BoundsExceeded` | candidate outside the governor's bounds | Investigate price OR adjust bounds (owner) |
| `DeltaExceeded` | jump > 20% in one epoch | Expected in a crash/pump; consider ForceSet |
| `env … missing` | `.env` not created/loaded | run `setup-env.sh`, check the unit |
