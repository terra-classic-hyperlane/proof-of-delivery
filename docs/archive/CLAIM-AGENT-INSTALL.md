# Installing the claim-agent on the VPS — automatic commission collection

The `claim-agent` **emits the receipts on its own** (every 5 min) on every live chain, and
the commissions land in your **operator wallets**. It runs on the VPS as a `systemd`
service, signing with a **dedicated trigger wallet** (which only pays gas). Your real
key **never goes to the VPS**.

## Concept: trigger wallet
- It is a **new, disposable** wallet, with **just a little gas** (BNB on BSC, LUNC on TC).
- It **signs** the `sendReceipt`/`send_receipt` and pays the gas.
- The **commission ALWAYS lands in your operator wallet** (`terra1run…` / `0x8f08…`),
  because the contract pays the address in the **from/to registry**, not whoever signed.
- If the VPS or the trigger wallet leaks, the attacker **only gets the gas change** — your
  real keys and your commissions stay intact.

## What is ALREADY installed (done)
On the VPS `31.97.91.4` (`~/claim-agent/`):
- `claim-agent-receipt.mjs` (the agent, without `terrad` — uses cosmjs + ethers).
- `solana-epoch-reporter.mjs` (the TC→Solana quorum reporter — see §final).
- `node_modules` → symlink to the `oracle-agent`'s.
- `.claim-agent-seen.json` (local state, pre-seeded).
- `/etc/systemd/system/claim-agent.service` (**enabled**, stopped).
- `.env` (template, `chmod 600`).

## What YOU do (2 steps)

### Step 1 — create and fund the trigger wallets
- **BSC:** create a **new** EVM wallet (e.g. `cast wallet new`, or MetaMask). Take
  the **hex private key `0x…`**. Send **~0.05 BNB** to its address.
- **TC:** create a **new** Terra wallet. Take the **mnemonic** (or the 32-byte hex
  key). Send **some LUNC** (e.g. 200 LUNC) to its address.

> Tip: after putting the keys in the `.env` (step 2), the agent **prints the trigger
> wallet addresses** in the log ("send BNB/LUNC for gas: …") — use it to fund them.

### Step 2 — set the keys and start
```bash
ssh root@31.97.91.4
nano ~/claim-agent/.env
```
Fill in (trigger wallet, **not** the real one):
```
BSC_PRIVATE_KEY=0xYOURHEXKEY_OF_THE_BSC_TRIGGER
TC_PRIVATE_KEY=hex_key_32bytes_of_the_TC_trigger
#   (alternative for TC:  TC_MNEMONIC=word1 word2 ... )
```
Start and follow along:
```bash
systemctl start claim-agent
tail -f ~/claim-agent/logs/agent.log      # or: journalctl -u claim-agent -f
```
You will see the rounds every 5 min, the trigger wallet addresses and the `txhash` of
the emitted receipts. The commissions arrive in the operator wallets shortly after (the
native relayer delivers the receipt back and the origin pays).

## Operation
```bash
systemctl status claim-agent      # state
systemctl restart claim-agent     # after editing the .env
systemctl stop claim-agent        # pause
tail -n 100 ~/claim-agent/logs/agent.log
```

## Test without keys (DRY — read only)
```bash
cd ~/claim-agent && DRY=1 node claim-agent-receipt.mjs
```
Shows what it would emit (how many pending per chain), without signing anything.

## Tuning (env in the `.env` or in the service)
- `--loop 300` → interval in seconds (default 5 min).
- `MIN_BATCH=3` → only emits when ≥ N deliveries from the same origin accumulate (amortizes gas).
- `DISPATCH_PAGES=100` → how many recent dispatches to scan.
- Receipt IGP: **quoted dynamically** (since 2026-08-20 — no fixed value).
  The agent queries `quote_gas_payment` on the TC IGP for the real delivery gas
  (`RECEIPT_GAS_56=300000` / `RECEIPT_GAS_SOL=500000`, tunable in the `.env`) and
  attaches the quote +2%; the `send_receipt` passes this `gas_limit` via metadata, so
  the receipt pays only the real gas (~100 LUNC for BSC, ~20 for SOL) and not the full
  user fee ($0.08). Details: `FEES-AND-REWARDS.md`.

## Security
- `.env` is `chmod 600` (only root reads it).
- The trigger wallet **only has gas**; recharge it when it empties.
- The commission never passes through the trigger — it lands directly in the operator wallet of the from/to.
- The agent does **not** deliver messages (that is the native relayer) and does **not** touch a
  native contract — it only fires the receipts, which anyone could fire.

## Corridors covered
- **TC→BSC** → `sendReceipt` on BSC (BNB gas) → commission in **LUNC on TC**.
- **BSC→TC** → `send_receipt` on TC (LUNC gas) → commission in **BNB on BSC**.
- **Solana→TC** → `send_receipt` on TC (LUNC gas) → commission in **SOL on Solana**.
- **ETH** → automatic once the ETH vault exists.

---

## Quorum reporter (TC→Solana) — INSTALLED as a service
The `~/claim-agent/solana-epoch-reporter.mjs` reports the **TC→Solana** deliveries (quorum
model — see `CLAIMS-AUTOMATION.md`). It runs as the **`epoch-reporter.service`** service
(systemd), loading the relayer's `.env` (`EnvironmentFile=/root/hyperlane/.env`) and
signing with the relayer's `SOLANA_PRIVATE_KEY` — which is the **PbEo** operator (registered),
so the `SubmitEpochReport` is accepted. Every hour it reports the newly closed epoch;
an already-reported epoch → "nothing to do" (idempotent).
```bash
systemctl status epoch-reporter
tail -f /root/claim-agent/logs/reporter.log
# manual/DRY:
cd ~/claim-agent && node solana-epoch-reporter.mjs           # shows the report without sending
```
**Pending adjustment:** `reward_lamports` is set to **1** (placeholder) — the reward per
TC→Solana delivery. Once you define the value (`pod` governance), future reports credit
that value; the operator withdraws the credits with the `pod`'s `Withdraw` instruction (I
can automate the withdrawal too, if you want).
