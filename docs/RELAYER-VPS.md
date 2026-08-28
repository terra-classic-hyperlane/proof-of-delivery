# Official Hyperlane relayer on the VPS — version, update and configuration

The transport of ALL cross-chain messages (transfers **and** receipts) is
handled by the **official Hyperlane relayer, with no code modification whatsoever** —
as the spec requires (§ "not a single line of the Hyperlane core is modified"). Our
offline agents (`oracle-agent`, `claim-agent`, `epoch-reporter`) only perform the
roles the relayer does NOT: price/gas, emitting the claims (receipts), and the
Solana epoch quorum. `deliver-receipts-tc.mjs` is just a **safety net**
(plan B, off by default — see `TRUSTLESS-RECEIPT.md`).

VPS `31.97.91.4` · service `hyperlane-relayer.service` · binary at
`/root/hyperlane/bin/relayer`.

## Versions

| | Version | Commit | Source | Binary date |
|---|---|---|---|---|
| **Previous** | local build | `906921a706a01b1d28a4936b06088f7cfa296851` | compiled locally from `~/hyperlane-monorepo` | 2026-06-04 |
| **Current** | **agents-v2.0.0** | `c117895a17dc5a932bc2007c15c53be26014e22d` | **official image** (not recompiled) | 2026-01-07 |

The previous binary (109 MB) had two defects specific to the build (not to our
code): rejection of the Cosmos broadcast in CheckTx (the SAME tx via cosmjs passed)
and file descriptor leakage. Replaced by the **official** v2.0.0 artifact
(114 MB) — updating is not modifying code, it is swapping an old official binary
for a new one.

## How it was done (extracting the official binary, without recompiling)

The canonical and reproducible way is to take the binary **from the official Docker
image** published by Hyperlane, instead of compiling (which on the VPS hits the
toolchain issue, rustc 1.84 vs edition2024 of the deps). Steps executed on the VPS:

```bash
# 1. official image (gcr.io/abacus-labs-dev/hyperlane-agent)
docker pull gcr.io/abacus-labs-dev/hyperlane-agent:agents-v2.0.0
#    verified digest:
#    sha256:e953983fee85fd01432f9e6a40e192cafc2c39db4a180aac34e55f8f624c964a

# 2. extract the relayer binary from the image (does not run the container)
C=$(docker create gcr.io/abacus-labs-dev/hyperlane-agent:agents-v2.0.0)
docker cp $C:/app/relayer /root/hyperlane/bin/relayer-v2.0.0
docker rm $C
chmod +x /root/hyperlane/bin/relayer-v2.0.0

# 3. backup of the old binary (rollback) + swap
cp /root/hyperlane/bin/relayer /root/hyperlane/bin/relayer-906921a7-backup
systemctl stop hyperlane-relayer
until ! ss -tlnp | grep -qE ':(9090|9091)\b'; do sleep 1; done   # wait for port to free
cp /root/hyperlane/bin/relayer-v2.0.0 /root/hyperlane/bin/relayer
systemctl start hyperlane-relayer
```

**Rollback** (if needed): `cp /root/hyperlane/bin/relayer-906921a7-backup
/root/hyperlane/bin/relayer && systemctl restart hyperlane-relayer`.

## CONFIGURATION changes (none in code)

1. **`metricsPort: 9091`** in `config/relayer.mainnet.json` — **the cause of the 3
   `AddrInUse` panics / zombie relayer**: in v2.0.0 the agent's server reads the
   `metricsPort` key (default **9090**) and IGNORES the legacy `--metrics 0.0.0.0:9091`
   from ExecStart; 9090 already belongs to the **validator** → the server panics on
   bind and this **kills the message processors** (the relayer was indexing but not
   delivering — that was why receipts got stuck). Fixed by pointing the relayer to
   9091 (the validator stays on 9090).
2. **`relayApiEnabled: false` / `relayApiPort: 9092`** — disables the relayer's HTTP
   control API (we don't use it) and, if it is ever turned on, keeps it on its own port.
3. **`LimitNOFILE=1048576`** (drop-in `.../hyperlane-relayer.service.d/limits.conf`)
   — raised file descriptor ceiling (the old binary leaked fds; kept as a safety
   margin).
4. **RPCs** in `config/agent-config.mainnet.json`:
   - BSC: official dataseeds first (they serve `eth_getLogs`) + publicnode/1rpc/drpc
     as backup; `index.chunk = 50` (the public ones limit getLogs to ≤50 blocks).
   - Solana: **Helius** (`mainnet.helius-rpc.com`, own key) ahead of the public
     `api.mainnet-beta`.
   - Terra Classic: hexxagon + publicnode + binodes.

> Operational rule (shared key): the account `terra1run9wz…` signs for the
> relayer, the claim-agent and manual scripts. After running any script that
> signs with it (migrate, igp-tariff, unenroll…), **restart the relayer** to
> resync the sequence: `systemctl restart hyperlane-relayer`.

## Post-update verification

```bash
systemctl is-active hyperlane-relayer                       # active
journalctl -u hyperlane-relayer -n 200 | grep -c panicked   # 0
ss -tlnp | grep -E ':(9090|9091)'                           # 9091=relayer 9090=validator
journalctl -u hyperlane-relayer | grep 'starting up with version'  # c117895a…
```
Functional proof: a new transfer should be delivered at the destination without
intervention (`delivered()`/`message_delivered` turns true and the commission is paid).

## Monitoring panel

`node deploy/monitor.mjs` — single health view (4 chains + VPS):
- **Trigger wallets**: TC operator, BSC 0x8f08, ETH 0xEF81, SOL PbEo (reporter),
  SOL BirXd4Q (authority) — with a ⚠ LOW alert when gas runs out.
- **Pools**: TC vault + accumulated IGP (warns when to sweep with Sweep), BSC vault,
  SOL pod/Config + base of the replay bitmap vs current epoch.
- **VPS services**: relayer/validator/oracle-agent/claim-agent/epoch-reporter/
  deliver-receipts.timer (active?), relayer panics and the number of fds.

```bash
node deploy/monitor.mjs            # full (on-chain + SSH into the VPS)
node deploy/monitor.mjs --no-vps   # on-chain only (no SSH)
node deploy/monitor.mjs --watch    # refreshes every 60s
```

### WEB panel (browser)

`node deploy/monitor-web.mjs` — starts a local server and shows the same panel in
the browser, auto-refreshing every 30s. The queries (RPC + SSH) run on the
server; the page only reads from localhost (nothing goes to the internet).

```bash
node deploy/monitor-web.mjs           # opens http://localhost:8787
PORT=9000 node deploy/monitor-web.mjs # another port
node deploy/monitor-web.mjs --no-vps  # without SSH
```
Leave it running in a terminal and keep the tab open.

### Web panel as a SERVICE (starts on its own)

`bash deploy/install-monitor-web-service.sh` — installs the panel as a systemd
user service (runs as you, with your SSH keys), enables it on boot
(linger) and keeps http://localhost:8787 always up.

```bash
systemctl --user status  tcpod-monitor    # state
systemctl --user restart tcpod-monitor    # after editing monitor-web.mjs
systemctl --user stop    tcpod-monitor    # stop
journalctl --user -u tcpod-monitor -f     # logs
```

The web panel (http://localhost:8787, real-time via SSE) shows 8 blocks in real time:
**Operators** (who they are + quorum of each oracle/vault of the 4 chains), **TC validators** (the 4, index of the signed checkpoint vs tip, 3-of-4 badge),
**RPCs** (height + latency of the 4 chains), **Messages** (queued/stuck + processed
+ cursor per chain, from the relayer), **Epochs & Oracle** (current epoch + when it closes,
next from the epoch-reporter, last/next from the oracle-agent + prices it
wrote on-chain), **Wallets**, **Pools** and **Services**.
