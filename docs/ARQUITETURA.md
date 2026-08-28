# Architecture — the whole picture

Diagrams of the complete system: the end-to-end Hyperlane process and where the
remuneration layer (this repository) fits in. The diagrams render on
GitHub (Mermaid). Normative details: `SPEC.html`.

---

## 1. Overview — the 4 networks and the two layers

The **Hyperlane core** (blue) is not modified; the **remuneration layer**
(green) only changes configuration: the `beneficiary` of each IGP becomes the Vault and, on
Solana, the `owner` of the IGP becomes the governor.

```mermaid
flowchart TB
    subgraph TC["🌕 TERRA CLASSIC (real collateral · on-chain governance)"]
        direction TB
        TCmb["Mailbox<br/>(writes sender+block into DELIVERIES)"]
        TCigp["IGP<br/>claim only for beneficiary"]
        TCora["StorageGasOracle<br/>(separate contract)"]
        TCwarp["Warp Route<br/>LUNC collateral"]
        TCvault["🟢 RelayerRewardVault<br/>proof by RAW QUERY"]
        TCgov["🟢 OracleGovernor<br/>quorum+median+bounds"]
        TCigp -- "beneficiary = vault" --> TCvault
        TCvault -- "raw query DELIVERIES" --> TCmb
        TCgov -- "oracle owner" --> TCora
    end

    subgraph EVM["🟡 BSC · 🔵 ETHEREUM (synthetic · multisig)"]
        direction TB
        Emb["Mailbox v3<br/>public processor(id)"]
        Eigp["IGP<br/>permissionless claim()"]
        Eora["StorageGasOracle"]
        Ewarp["synthetic Warp"]
        Evault["🟢 RelayerRewardVault.sol<br/>proof by processor()"]
        Egov["🟢 GasOracleGovernor.sol"]
        Eigp -- "pushes balance (receive)" --> Evault
        Evault -- "processor(id)" --> Emb
        Egov -- "oracle owner" --> Eora
    end

    subgraph SOL["🟣 SOLANA (synthetic · multisig · NO executor record)"]
        direction TB
        Smb["Mailbox<br/>ProcessedMessage without executor"]
        Sigp["IGP<br/>(oracle INSIDE the Igp)"]
        Swarp["synthetic Warp"]
        Svault["🟢 rrv: PDA config = POOL<br/>quorum by EPOCH"]
        Sgov["🟢 IgpOracleGovernor<br/>two doors · IGP owner"]
        Sigp -- "beneficiary = vault PDA" --> Svault
        Sgov -- "owner (CPI signed by the PDA)" --> Sigp
    end

    TCwarp <-. "Hyperlane messages<br/>(validators + relayers)" .-> Ewarp
    TCwarp <-. " " .-> Swarp
```

---

## 2. The complete Hyperlane process (with remuneration attached)

A TC → BSC transfer, from dispatch to the relayer's withdrawal — steps 1–8 are
the **unchanged core**; 9–11 are the **new layer**:

```mermaid
sequenceDiagram
    autonumber
    actor U as User
    participant WO as Warp Route (TC)
    participant MO as Mailbox (TC)
    participant IO as IGP (TC)
    participant V as Validators (sign checkpoints)
    actor R as Relayer (anyone!)
    participant MD as Mailbox (BSC)
    participant ISM as ISM (BSC)
    participant WD as synthetic Warp (BSC)
    participant VD as Vault (BSC)

    U->>WO: transfer(1000 LUNC, dest=BSC)
    WO->>IO: quote + payForGas (user's LUNC)
    Note over IO: proceeds stay in the IGP<br/>until someone pulls them to the beneficiary
    WO->>MO: dispatch(message)
    MO-->>V: new checkpoint (merkle root)
    V-->>V: sign the checkpoint (threshold e.g.: 3-of-4)
    R->>MD: process(metadata, message)
    MD->>ISM: verify(signatures)
    ISM-->>MD: ✓ valid
    MD->>WD: handle() → mints synthetic for the recipient
    Note over MD: Mailbox writes Delivery{processor=R, block}
    R->>VD: claim([message_id])
    VD->>MD: processor(id) == R? processedAt+window ok?
    VD-->>R: 💰 reward in BNB (from the pool)
```

The return path (BSC → TC) is mirrored: the user pays BNB in the BSC IGP, the
relayer spends LUNC in the TC `process()` and withdraws from the **TC Vault** — each network
sustains itself on its own coin (spec §05).

---

## 3. Money flow (self-funded by construction)

```mermaid
flowchart LR
    U1["Users that dispatch<br/>FROM network X"] -- "payForGas<br/>(X's coin)" --> IGP["IGP of network X"]
    IGP -- "claim()/Sweep<br/>(entire balance)" --> POOL["🟢 Vault of network X<br/>(pool in X's coin)"]
    POOL -- "reward × proven<br/>deliveries" --> REL["Relayers that DELIVERED<br/>ON network X"]
    GOVx["governance/multisig"] -. "WithdrawSurplus /<br/>fee / window" .-> POOL
    style POOL fill:#0a6b4e,color:#fff
```

No usage → no proceeds, but also no work. Fee < average proceeds
per delivery ⇒ the pool never becomes insolvent (spec §01/§05).

**v2 (ClaimRemote):** the ORIGIN network's pool also pays the message fee to the
operator that delivered it ON ANOTHER network — via quorum attestation (diagram 4b):

```mermaid
flowchart LR
    U2["User dispatches<br/>FROM X to Y"] -- "fee (X's coin)" --> IGPX["IGP of X"] --> POOLX["Vault of X"]
    RELY["YOUR relayer<br/>delivers on Y"] -- "verified by the<br/>claim-agent" --> ATT["AttestRemoteDelivery<br/>on X's Vault"]
    ATT -- "binding + quorum +<br/>1x per message_id" --> POOLX
    POOLX -- "remote reward<br/>(≈ origin fee)" --> OP["Operator<br/>(address on X)"]
    style POOLX fill:#0a6b4e,color:#fff
```

---

## 4. Proof of delivery — the three mechanisms

```mermaid
flowchart TB
    subgraph P1["TERRA CLASSIC · proof by raw query"]
        A1["Relayer calls Vault::Claim{ids}"] --> B1["raw query: key<br/>[0x00,0x0A]+'deliveries'+id"]
        B1 --> C1{"value parses<br/>{sender, block_number}?"}
        C1 -- "no" --> D1["⛔ MailboxLayoutMismatch<br/>(migrate detected — never pays wrong)"]
        C1 -- "yes" --> E1{"sender == relayer?<br/>within the window?<br/>not paid yet?"}
        E1 -- "yes (ALL ids)" --> F1["💰 BankMsg (atomic batch)"]
        E1 -- "no" --> G1["⛔ reverts the WHOLE BATCH"]
    end

    subgraph P2["BSC/ETHEREUM · direct proof"]
        A2["claim(ids)"] --> B2["mailbox.processor(id) == msg.sender?<br/>processedAt(id)+window ≥ block?"]
        B2 -- yes --> C2["💰 native transfer (atomic, reentrancy-guard)"]
    end

    subgraph P4["ANY ORIGIN · v2 ClaimRemote (REMOTE delivery attestation)"]
        A4["claim-agent verifies the delivery<br/>on the DESTINATION chain"] --> B4["AttestRemoteDelivery{domain, ids}<br/>on the ORIGIN chain's vault"]
        B4 --> C4{"attester registered?<br/>binding (operator,domain)?<br/>id never paid?"}
        C4 -- yes --> D4{"AGREEING attestations<br/>≥ quorum?"}
        D4 -- yes --> E4["💰 fixed reward for the domain<br/>to the bound operator (1x per id)"]
        D4 -- "not yet" --> F4["attestation recorded,<br/>awaits quorum (auditable)"]
        C4 -- no --> G4["⛔ reverts"]
    end

    subgraph P3["SOLANA · quorum by epoch (the chain does not record the executor)"]
        A3["6h epoch closes<br/>(+ finality slack)"] --> B3["each operator submits the report:<br/>window + credits ORDERED by key"]
        B3 --> C3{"identical hashes<br/>≥ quorum?"}
        C3 -- yes --> D3["credits assigned per operator<br/>(PDA OperatorCredit)"]
        C3 -- "divergence" --> E3["🚨 epoch locked — alarm,<br/>public audit of the reports"]
        D3 --> F3["Withdraw: lamports debit<br/>from the pool (respects rent-exempt)"]
    end
```

---

## 5. Governance org chart (spec §04)

Three spheres with non-overlapping scopes — the operational with whoever operates, the
structural with whoever holds a mandate:

```mermaid
flowchart TB
    GOV["🏛️ TERRA CLASSIC GOVERNANCE<br/>on-chain proposal<br/><i>scope: EVERYTHING within TC —<br/>IGP · ISM · Vault · Oracle · fee · bounds</i>"]
    MS["🔐 MULTISIG (BSC · ETH · SOL)<br/>Hyperlane validators + EXTERNAL signers<br/><i>scope: IGP · ISM · oracle bounds ·<br/>upgrade authority (Solana) · emergencies</i>"]
    OPS["⚙️ RELAYER OPERATORS<br/>on-chain quorum (no wallet multisig)<br/><i>scope: price WITHIN the bounds · epoch<br/>reports (SOL) · remote vault parameters</i>"]
    ALL["🌐 ANYONE (permissionless)<br/><i>deliver messages · withdraw one's OWN<br/>reward · Sweep · audit everything</i>"]

    GOV -- "grants mandate via<br/>approved proposal" --> MS
    MS -- "defines bounds and locks<br/>(never the operators)" --> OPS
    OPS -.-> ALL
    style GOV fill:#B58CFF,color:#000
    style MS fill:#F0B90B,color:#000
    style OPS fill:#3FD0C9,color:#000
```

> ⚠️ The project's #1 risk lives here: whoever controls the **remote Warp's ISM**
> has indirect access to the TC collateral (mint synthetic without backing → burn
> → legitimate message releases collateral). That is why the multisig can **never** be
> composed only of the validators that sign checkpoints, and the ISM swap requires a
> timelock (spec §12).

---

### 5b. ClaimRemote — who controls what (v2)

```mermaid
flowchart TB
    OWN["Vault owner<br/>(today deployer · later governance/multisig)"]
    OWN -->|SetRemoteOperators| ATT["Attesters + quorum<br/>(today: 1 operator, quorum 1)"]
    OWN -->|SetRemoteBinding| BIND["Identity bindings<br/>operator ↔ address on each chain"]
    OWN -->|SetRemoteReward| RW["Fixed reward per domain<br/>(≈ average origin fee)"]
    ATT -->|AttestRemoteDelivery| PAY["Payment 1x per message_id<br/>(effects-first, auditable)"]
    BIND --> PAY
    RW --> PAY
    NOTE["⚠️ with 1 operator, quorum 1 is self-attestation<br/>(test phase) — raise to ≥2 with independent operators"]
    ATT -.-> NOTE
```

## 6. Price oracle — the same pattern across the 4 networks

```mermaid
flowchart LR
    subgraph DEFINE["governance (TC) / multisig (remotes)"]
        FX["bounds [min,max] per domain<br/>+ max variation (bps)<br/>+ token_decimals (SOL)"]
    end
    subgraph OPS["operators (each with its oracle-agent)"]
        OA["agent A"] & OB["agent B"] & OC["agent C"]
    end
    G["🟢 GOVERNOR<br/>median (lower of the central ones)<br/>within bounds? Δ < limit?<br/>1 application per epoch"]
    ORC["network oracle<br/>(StorageGasOracle / Igp's gas_oracles)"]
    QUOTE["IGP quotes<br/>(what the user pays)"]

    FX -- "on-chain lock" --> G
    OA & OB & OC -- "SubmitPrice<br/>(independent observation)" --> G
    G -- "CPI / setRemoteGasData" --> ORC --> QUOTE
    DEFINE -. "EMERGENCY: direct write<br/>+ return of ownership" .-> ORC
    style G fill:#0a6b4e,color:#fff
```

The conflict of interest (operators control the price that funds their own
remuneration) is neutralized because the **bounds are defined by whoever does not operate**.

---

## 7. Solana — the two doors of the IgpOracleGovernor

On Solana the oracle lives INSIDE the `Igp` and only the owner writes — the governor becomes
the owner and rebuilds the separation of powers:

```mermaid
flowchart TB
    subgraph GOVSOL["🟢 IgpOracleGovernor (Igp owner via config PDA)"]
        P1["DOOR 1 · operators<br/>SubmitPrice → quorum → median<br/>→ bounds/delta → CPI"]
        P2["DOOR 2 · multisig (1 signature)<br/>SetDomainConfig (bounds+decimals)<br/>SetIgpBeneficiary<br/>ForceSetGasData<br/><b>TransferIgpOwnership ← emergency exit</b>"]
    end
    IGPS["Igp { owner, beneficiary, gas_oracles }"]
    P1 -- "SetGasOracleConfigs<br/>(signed by the config PDA)" --> IGPS
    P2 -- "administrative CPIs" --> IGPS
    MS2["multisig"] --> P2
    MS2 -- "program's upgrade<br/>authority" --> GOVSOL
```

The separation becomes **logical** (governor code), not cryptographic —
which is why the three obligations of spec §08: `TransferIgpOwnership` tested before
the deploy (✅ tested in `svm/.../tests`), lamports kept in the config PDA, and
upgrade authority in the multisig.
