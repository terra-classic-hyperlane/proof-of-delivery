# Warp Route IGORFAKE — full map (on-chain, 08/18/2026)

Reference for all addresses, IGPs, oracles, ISMs+validators, and prices of the
**IGORFAKE** warp (cw20 collateral on Terra Classic ↔ synthetic on BSC/ETH/Solana).
Collected directly from the chain — the sources of truth are the contracts, not the config.

Token: **IGORFAKE** · 6 decimals · TC = collateral (cw20) · remotes = synthetic.

---

## Terra Classic (columbus-5, domain **132556**) — COLLATERAL SIDE

| Piece | Address |
|---|---|
| **Warp (cw20 collateral router)** | `terra1wr7krp8lpfddpzxfkxvmhfnxd06vkz34e7f0tk2vyau36j3d4pvs6pjpel` |
| token cw20 (real collateral) | `terra1lpkaaqjaq8zfwktge3vy0zg46nxxsynsge2wxa7addpweu2w6gmsy3lhkr` |
| Mailbox | `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9` |
| IGP | `terra1taunhg629rssf3g939nqr0h594q5mssrzdj5lkx2hygmxmh72ghqeqqnvz` |
| IGP Oracle | `terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d` |
| ISM Routing | `terra1uhzzvt9x3u8hjnkp695hklexx2uywjvfqv454d93ds92sgtpwk7qrpxdg0` |
| Owner (all) | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` |

- cw20 `total_supply`: **999999999998810400** (≈ 10¹²·10⁶); locked collateral backing the synthetics.
- **IGP Oracle — current prices (what the TC user pays per destination):**

| Destination domain | exchange_rate | gas_price |
|---|---|---|
| 1 (Ethereum) | 376 | 10000000000 (10 gwei) |
| 56 (BSC) | 1098 | 3000000000 (3 gwei) |
| 1399811149 (Solana) | 383001553014 | 1 (lamport model) |

- Enrolled routes (remote routers, hex 32B): dom 1 `…a687a4c4…`, dom 56
  `…3605d894…`, dom 1399811149 `c6de5b1f…437f95`, dom 1399811151 (SOL testnet) `db7c50f8…`.

### Multisig ISM per domain (OFFICIAL Hyperlane validators that sign for TC)

| ISM (for msgs coming from) | Address | Threshold | Validators |
|---|---|---|---|
| Ethereum (dom 1) | `terra187rzjc3dznfxqtqqrwh796e5q4khmvp5av8mka6zhp98zjfk2z2qneldar` | **6 of 9** | see TC deploy guide |
| BSC (dom 56) | `terra1nqj7qlnt2sty0dgnu3ss5z4u6wr7hjfea7cn6wpwjt2uymts8ucsmuj9xw` | **4 of 6** | same |
| Solana (dom 1399811149) | `terra10s3p36tjek8amhlc4krxpzln6g8n0qy9jq82wyda434l3rv89wfsucl50t` | **3 of 5** | same |

> These ISMs verify the ENTRY into TC (messages coming from the remotes). The
> `proof-of-delivery` does NOT change them — it only starts reading the Mailbox to pay the relayer.

---

## BSC (domain **56**) — SYNTHETIC SIDE

| Piece | Address |
|---|---|
| **Warp (synthetic router)** | `0x3605D8946FC6F5A75d89d92173100F59743B5318` |
| Mailbox | `0x2971b9Aec44bE4eb673DF1B88cDB57b96eefe8a4` |
| IGP (custom TerraClassicIGPStandalone) | `0xEdEd7a4f6FEe4B474B9d7730Bf3465E35E2a4923` |
| Oracle (TerraClassicOracle) | `0x7dE950f8F0a037783989a6BE84B3620916552306` |
| ISM (StorageMessageIdMultisigIsm, MUTABLE) | `0xF6b0cDD33A7d2895a3F18b85569Ed9A8278cD151` (final since 08/20/2026 — see `ISM-VALIDATORS.md`) |
| Owner (warp/IGP/oracle/beneficiary) | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` |

- synthetic `totalSupply`: **3688000000** (3,688 IGORFAKE, 6 dec).
- **ISM validators:** 4 validators (igorveras/tcv/darksun/burnitall) · **threshold 3** —
  see `ISM-VALIDATORS.md` (validates msgs coming from TC).
- **Oracle (what the BSC user pays to send to TC):** rate `9047190` · gas_price `1e10`.

---

## Ethereum (domain **1**) — SYNTHETIC SIDE

| Piece | Address |
|---|---|
| **Warp (synthetic router)** | `0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5` |
| Mailbox | `0xc005dc82818d67AF737725bD4bf75435d065D239` |
| IGP (custom) | `0x9650F1f8DB492750323172145e67Df4e89E964Aa` |
| Oracle | `0x3987cCE8f08037EBF93Ef3a934753540A94196cE` |
| ISM (StorageMessageIdMultisigIsm, MUTABLE) | `0x3ba17675f0D319C89D70722f6eb07790DF0B254B` (final since 08/20/2026 — see `ISM-VALIDATORS.md`) |
| Owner (warp/IGP/oracle/beneficiary) | `0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae` |

- synthetic `totalSupply`: **1308000000** (1,308 IGORFAKE).
- **ISM validators:** 4 validators (igorveras/tcv/darksun/burnitall) · **threshold 3** — see `ISM-VALIDATORS.md`.
- **Oracle:** rate `26585078` · gas_price `1e10` (10 gwei).

---

## Solana (domain **1399811149**) — SYNTHETIC SIDE

Source: deploy log `WARP-SOLANAMAINNET-IGORFAKE-BUFFER.txt` + `REFERENCE-IGP`
(07/09) + real dispatch tx (below).

| Piece | Address |
|---|---|
| **Warp/router (program)** | `EPJNrrpCeZGqDPoFtdV9u9uDWBNW3Xqh84LfM7345zcL` (hex `c6de5b1f…437f95` = the route on TC) |
| Mint (synthetic token) | `CeLHx5Wm9AzuWRnP4URMfNqNa9kDDrnsNGoATCS96QwD` |
| Mailbox (sealevel) | `E588QtVUvresuXq2KoNEwAmoifCzYGpRBdHByN9KQMbi` |
| **IGP program** | `FLZuKRsfdovLqd8n1AYhPCwLqBjfFyZY3A2edgnjdJoR` (binary sha256 `4321c426…08c6d`) |
| IGP account (inner — RECEIVES the payment) | `FPTvDsowMHXFKktoLgy2a2qfr5yL6846JHKwvk2mYKFk` |
| Overhead IGP (the one the WARP uses) | `FXacR73HiuNyvW7x34KYCDyv8XxM86pz31Ap8t2v3RCJ` |
| ISM (MultisigISM program) | `4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ` (mainnet ID `LwNfVYMDzAe5dCJgA5CipTZcT34Eyf74zLr81K91jxk`) |
| Owner / upgrade authority | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` |

- **RemoteGasData for TC (132556) in the IGP:** exchange_rate `29400000000` · gas_price
  `28325` · token_decimals `6` · gas_overhead `3000000` (the deploy reads the CURRENT
  on-chain value; these are the 07/09 reference values).
- ISM: 4 validators / threshold 3 since 08/20/2026 (see `ISM-VALIDATORS.md`). There is also a route to **Solana testnet** (dom 1399811151).

> ⚠️ Two IGP accounts: the **inner `FPTvDso…`** is the one that accumulates the lamports (it is
> the one that receives `beneficiary = vault` and `owner = governor`); the **overhead
> `FXacR73…`** is the one the warp references. The `solana-init.mjs` already uses `FPTvDso…`.

### Proof of the full cycle (real tx)

`4wiG4TtZDFgvzY1wWYTkTDr8J64svrDZD4qShLYk5VQXKchEM1MCGoCw11X9ZyBzuZcztqFVHQymrChDX5Q6fpX1`
(slot 431826035): burn of the synthetic → `Dispatched message to 132556, ID
0xd039daa1c75d5b558906fef6d790b13d…` → `Paid IGP FPTvDsow… for 6000000 gas`.
This is **the same message** that the relayer `terra1run9wz…` delivered on TC (block
29422362) and that the vault validates via `layout_check` (`ok:true`) — the
SOL→TC→proof-of-delivery flow closed end to end.

> There is an EARLIER deploy (program `Behbk6ULj6PjvN9ZTb5vesyfor2hFhkw6y131UCRtgPx`,
> mint `HHVv9R48…`, IGP `BhNcatUDC2D…`, domain 1325/testnet) preserved in the logs —
> it is NOT the production one. The production one is `EPJNrr…` above.

---

## Where the proof-of-delivery connects

On each leg, the **IGP** in this table receives `beneficiary = Vault` and the **oracle**
passes to the **governor** (owner). On TC this is ALREADY DONE (Phases 1–2 — see
`AUDIT-TC.md`): IGP `terra1taunhg…` → beneficiary vault `terra1gqkrh2…`,
oracle `terra1j8xz…` → owner governor `terra1z7jmlky…`. On the remotes, the scripts
`evm-deploy.sh`/`solana-deploy.sh` do the same with the addresses above.
