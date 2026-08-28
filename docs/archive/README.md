# Archive — historical records (superseded, kept for provenance)

These documents are **dated snapshots**: deploy audit records from the 2026-08-18
launch, installation reports, and guides that have since been superseded. They are
kept unchanged because they are part of the system's audit trail (deploy provenance,
original tx hashes, phase-by-phase decisions) — but they **do not describe the
current state**.

For the current state, use the two entry-point documents:

- **[`../install/AUDIT.md`](../install/AUDIT.md)** — consolidated audit: current
  addresses, hashes on every chain, powers, security model, verification commands.
- **[`../install/INSTALL.md`](../install/INSTALL.md)** — operator install guide
  (oracle-agent · claim-agent · epoch-reporter) with architecture diagrams and the
  one-shot installer.

| Archived document | What it was | Superseded by |
|---|---|---|
| `AUDIT-TC.md` / `-BSC` / `-ETH` / `-SOLANA` | per-network deploy audit records (2026-08-18, original tx hashes) | `install/AUDIT.md` |
| `AUDIT-LOG.md` | consolidated snapshot of addresses/parameters (2026-08-18) | `install/AUDIT.md` |
| `AUDIT-COMMISSIONS.md` | on-chain commission test cases (launch period) | `install/AUDIT.md` + `../FEES-AND-REWARDS.md` |
| `MIGRATION-PLAN-EN-TRANSLATION.md` | i18n migration plan (executed) | `../I18N-AUDIT-REPORT.md` |
| `ORACLE-AGENT-INSTALL-REPORT.md` | production install report (2026-08-18) | `install/INSTALL.md` |
| `CLAIM-AGENT-INSTALL.md` | manual claim-agent install steps | `install/INSTALL.md` + `deploy/install-operator.sh` |
| `OPERATOR-INSTALL-RELAYER-ORACLE-VAULT.md` | consolidated full-node guide | `install/INSTALL.md` (services) + `../RELAYER-VPS.md` (relayer/validator) |
| `CLAIMS-AUTOMATION.md` | claim-agent + epoch-reporter concept guide | `install/INSTALL.md` + `../TRUSTLESS-RECEIPT.md` |
