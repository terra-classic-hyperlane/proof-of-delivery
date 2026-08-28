# ClaimRemote security — trust model and who decides the payment

Answers: "the relayer cannot decide who gets paid; that has to be in the contract".

## 1. Two different "beneficiaries" — don't confuse them

| Term | Who it is | Role |
|---|---|---|
| **IGP `beneficiary`** | **the VAULT contract** | receives the bridge's gas collection (it is where the IGP's `claim()` pushes the funds). It is money COMING INTO the pool. |
| **`executor` (ClaimRemote)** | an **operator** (bound address) | receives the origin fee reward for a remote delivery. It is money LEAVING the pool. |

The vault is the **vault**: gas enters it (as IGP beneficiary) and the reward
leaves it to the operator (as ClaimRemote payment). The relayer never touches
the vault — it only **triggers** functions whose rules are 100% in the contract.

## 2. What the CONTRACT decides (not the relayer)

The relayer/agent only EXECUTES the transaction. Each rule below is enforced by
the bytecode — modifying the agent does not bypass them:

1. **Recipient locked by allowlist.** The payment only goes to an address
   that the **owner/governance** bound (`remoteBinding`). A malicious agent
   CANNOT redirect it to a new attacker wallet — the tx reverts
   with `NoBinding`. Who decides the set of recipients is the owner, in the contract.
2. **1 payment per `message_id`** (`remote_claimed`, effects-first). Never pays
   the same message twice, even under reentrancy (guard) or a race between agents.
3. **Fixed cap per domain** (`remote_reward`). A forged id costs at most the
   domain's reward, never the whole pool. Only the owner changes the cap.
4. **Anti self-payment (quorum ≥ 2).** The attestation of the beneficiary ITSELF
   does not count toward the quorum: paying operator X requires `quorum`
   INDEPENDENT attesters (≠ X). A single key cannot pay itself. (EVM/CW: exclusion of one's
   own vote; Solana: `quorum` byte-identical reports from distinct operators.)
5. **Emergency pause** (`SetPause`) freezes all attestation.

## 3. The physical limit (and what it requires)

The ORIGIN chain **cannot see** the destination chain. Therefore "message X was
delivered over there by operator Z" is the ONLY claim the contract cannot
verify on its own — it comes from the attesters. This is irreducible without one of two things:

- **(today) Quorum attestation** — trust distributed among N independent
  operators. Safe if **quorum ≥ 2** and the operators are separate
  (distinct keys/machines). **With quorum = 1 (test phase, 1 operator) the
  single key is the authority — no contract removes this; it is the definition of 1
  operator.** That is why quorum = 1 is EXPLICITLY test-only.
- **(trustless target) Return receipt via Hyperlane** — the DESTINATION vault (which
  CAN verify the delivery on-chain, via `processor(id)`/DELIVERIES) dispatches a
  message back to the ORIGIN vault asserting "id X delivered by Z". That
  message passes through the SAME validator/ISM security of the bridge. The origin
  vault receives it through its Mailbox and pays — **without trusting any attester**;
  the contract determines the recipient from a message signed by the
  validators. Cost: the gas of one return message per delivery. This is the path
  to eliminate 100% of the trust — it is proposed, not implemented (a governance
  decision on cost/benefit).

## 4. Direct answer to the concern

> "Malicious code in the relayer is a problem."

True, and mitigated in layers: a compromised agent **holds the key of ONE
operator** — that is, it is equivalent to ONE malicious attester. Against this:
- it **cannot** pay an address outside the allowlist (rule 1);
- it **cannot** pay twice nor above the cap (rules 2–3);
- with **quorum ≥ 2** it **cannot** reach the quorum alone (rule 4) — it needs
  to compromise N independent operators at the same time.

The only scenario in which a single key has full authority is **quorum = 1**,
which exists only because today there is **1 operator in test**. The security
action for production is **operational and already supported by the contract**:
add independent operators and raise the quorum (`EXPANSION-MANUAL.md` §3). The
anti self-payment (rule 4) is already in the bytecode, ready for the moment the
quorum goes above 1.

## 5. State of the code

- Rule 4 implemented and tested: EVM 39 tests · CW 30 tests (`cargo`+`forge`
  green). Reproducible CW rebuild: sha256
  `ee8893da963bb2dd6eb20a6090f241e80c523f867a5b2a923baa5f601cce29d4`.
- **Deploy:** rule 4 is a NO-OP at quorum = 1 (idempotent with what is already
  live), so it does not require a redeploy for the test phase. It must be deployed
  **before** any change to quorum ≥ 2 (bundled with the multi-operator go-live):
  TC `migrate` + EVM redeploy + config.
