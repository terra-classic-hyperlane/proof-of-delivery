# Commissions Audit — on-chain test cases (all corridors)

> Real, on-chain verification of **who received the commission, how much, in which tx**, with
> **all messages in hex and decoded**. Made for analysis/audit.
> Collection date: 2026-08-19.

## 0. READ THIS FIRST — why you "couldn't find" the payment

The commission for a message **X → Y** is paid **on the ORIGIN chain (X), in X's currency**,
to the operator's address **on that chain** — **not** on the destination chain.

| You delivered a msg… | The commission lands on… | In the currency… | Proven below |
|---|---|---|---|
| **TC → BSC** (delivered on BSC) | **TC** | **LUNC** | §1 |
| **BSC → TC** (delivered on TC) | **BSC** | **BNB** | §2 |
| **Solana → TC** (delivered on TC) | **Solana** | **SOL** | §3 ✅ **works** |
| **TC → ETH / ETH → TC** | — | — | §4 (ETH vault not deployed) |
| **TC → Solana** (OPPOSITE direction of the one above) | — | — | §4 (not supported — do not confuse with Solana→TC, which works) |

If you delivered a **TC→BSC** message and went looking for the payment **on BSC**, you would
not find it: it is **on TC, in LUNC**. And vice versa. This is the most likely cause of the confusion.

### On-chain totals (confirm that payment occurred)
- **TC** `total_remote_paid` = **165,000,000 uluna = 165 LUNC** (query `remote_config`).
- **BSC** vault `0x34E06a77…` — payments in BNB recorded in `remoteClaimed` (§2).
- **Solana** — credited to the operator's PDA and withdrawn (§3).

---

## 1. Corridor TC → BSC  (origin TC pays **LUNC** on TC) ✅

**Original message** (IGORFAKE transfer leaving TC):
- `message_id`: `974a7e472521a652b55758550f3786d6f34cf3a01c9b1652ada4256b5c56ea8d`
- **hex**:
  ```
  0300000012000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000380000000000000000000000003605d8946fc6f5a75d89d92173100f59743b5318000000000000000000000000867f9ce9f0d7218b016351cb6122406e6d247a5e00000000000000000000000000000000000000000000000000000000002625a0
  ```
- **decoded**:
  | field | value |
  |---|---|
  | version | 3 |
  | nonce | 18 |
  | origin | `132556` (Terra Classic) |
  | destination | `56` (BSC) |
  | sender (warp TC) | `0x70fd6184…d4a2da859` |
  | recipient (warp BSC) | `0x…3605d8946fc6f5a75d89d92173100f59743b5318` |
  | **wallet receiving the token** | `0x867f9ce9f0d7218b016351cb6122406e6d247a5e` (BSC) |
  | **amount transferred** | `2,500,000` IGORFAKE units |

**Delivery**: operator index 0 (`terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`) delivered on BSC.

**Receipt → payment on TC** (tx `F4700EF49F734DEE8171C3BB93AEAC8EB1F0157B781BB6879CBAE1F381A4B126`, event `handle_receipt`, receipt origin = 56):

| **COMMISSION** | |
|---|---|
| Received by | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` (operator's TC wallet) |
| Amount | **33,000,000 uluna = 33 LUNC** |
| On-chain record | `remote_claimed[974a7e47…]` → `{claimed:true, executor:terra1run…, domain:56, amount:33000000, block:30019234}` |

> TC total = 165 LUNC ⇒ there are **~5 payments** of this type (33 LUNC each). The public RPC
> only indexes a recent window (we recovered 1 tx: `F4700EF4`); the rest are confirmed
> by the total and by `remote_claimed[<id>]` of each id (see §5).

---

## 2. Corridor BSC → TC  (origin BSC pays **BNB** on BSC) ✅

**Original message** (IGORFAKE leaving BSC):
- `message_id`: `5920d3fbf1d68e4cd3a5e0e4bb834ec83fc40d8e0f8ea2ef3530b3efe038ca84`
- **hex**:
  ```
  03000649c4000000380000000000000000000000003605d8946fc6f5a75d89d92173100f59743b5318000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000000000000000000000fedd34151143a14c158feb8cdeced2febaa0c1370000000000000000000000000000000000000000000000000000000000c65d40
  ```
- **decoded**:
  | field | value |
  |---|---|
  | version | 3 |
  | nonce | 412100 |
  | origin | `56` (BSC) |
  | destination | `132556` (Terra Classic) |
  | sender (warp BSC) | `0x…3605d8946fc6f5a75d89d92173100f59743b5318` |
  | recipient (warp TC) | `0x70fd6184…d4a2da859` |
  | **wallet receiving the token** | `0xfedd34151143a14c158feb8cdeced2febaa0c137` = `terra1lmwng9g3gws5c9v0awxdankjl6a2psfhm8pc8z` |
  | **amount transferred** | `13,000,000` IGORFAKE units |

**Delivery**: operator index 0 delivered on TC (tx `EA39249CBC0E11434DD70A575F015BD9DF38AE02F22D6210E42E055B31178370`).

**Receipt → payment on BSC** (vault `0x34E06a7793877EC5251b1dC230aD7cD577d231f4`):

| **COMMISSION** | |
|---|---|
| Received by | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` (operator's BSC wallet) |
| Amount | **2,259,538,750,000 wei = 0.00000225953875 BNB** |
| On-chain record | `remoteClaimed(5920d3fb…)` → `(0x8f08…, 132556, 2259538750000, block 116873093)` |

---

## 3. Corridor Solana → TC  (origin Solana pays **SOL** on Solana) ✅ *(proven in this round)*

**Original messages** (IGORFAKE leaving Solana — two, in a single receipt/batch):

**Msg A** — `message_id` `d5e2ab02bef59776d6c6dc43e1e566e15890c986f36e4fdebf2c5af37cdacc4f`
```
030005cc08536f6c4dc6de5b1fd8d285c06fa3967440530edfec35e907464599e3b485c5f273437f95000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da8590000000000000000000000003fc7ee49a59c1041d4a58bc21ef657eb443c8bbb00000000000000000000000000000000000000000000000000000000005b8d80
```
| field | value |
|---|---|
| origin → destination | `1399811149` (Solana) → `132556` (TC) |
| sender (warp Solana) | `0x536f6c4d…273437f95` |
| **receiving wallet** | `0x3fc7ee49…443c8bbb` = `terra18lr7ujd9nsgyr49930ppaajhadzrezam70j39k` |
| **amount transferred** | `6,000,000` IGORFAKE units |

**Msg B** — `message_id` `d039daa1c75d5b558906fef6d790b13dc94a8b39e58e1e7f219b3967a28c4f04`
```
030005cc01536f6c4dc6de5b1fd8d285c06fa3967440530edfec35e907464599e3b485c5f273437f95000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000000000000000000000fedd34151143a14c158feb8cdeced2febaa0c13700000000000000000000000000000000000000000000000000000000002dc6c0
```
| field | value |
|---|---|
| origin → destination | `1399811149` (Solana) → `132556` (TC) |
| **receiving wallet** | `0xfedd3415…baa0c137` = `terra1lmwng9g3gws5c9v0awxdankjl6a2psfhm8pc8z` |
| **amount transferred** | `3,000,000` IGORFAKE units |

**Delivery**: operator index 0 delivered on TC (txs `6B6BCA15…` and `4126C514…`).

**Receipt** (send_receipt on TC, tx `FD720251DAA642AC7EE65C36BC7AFB977BD4C9729007D82204AA9AE23CBF67A3`) →
receipt `5f67d0f7eec906e72bf724f1333b1657b6c924773ee88a6e33a62706a421158a` delivered on `pod`
(PDA `ProcessedMessage` `pFtaCoYr9UQaMLjVwD5SGp8KZeVDXnH8vqYxhDzmgZ6`).

| **COMMISSION** | |
|---|---|
| Credited to the PDA | `operator_sol(0)` = `8pz9ToVyJGcuF7enE4KERjQ9JG4My5vpm8XFvLwqer1j` |
| Amount | **998,000 lamports = 0.000998 SOL** (2 × 499,000) |
| **Withdrawn to** | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (operator's Solana wallet) |
| Withdrawal tx | `7mf9HE9Ck5fYqRg2XnLt9VoArFw3HBYUjhsZmsY2GLh5yk79mnDNy8XDaqsCdvQ18NiXwQFT8XYXLEGcMqUecU5` |

> On Solana the payment happens in **2 steps**: `handle` credits the operator's PDA and the
> operator **withdraws** (`WithdrawOperatorSol`). If you only look at the wallet, check **first**
> the PDA `operator_sol(index)` and the withdrawal tx.

---

## 4. Corridors WITHOUT commission (expected)

- **TC ↔ ETH** — the ETH receipt vault **has not been deployed yet** (waiting for low gas).
  Without a vault, there is no receipt and no payment. **No ETH commission exists** —
  it is not a bug, it is a pending step. (Warp ETH ISM at the time: `0xDe8edEC7…`; since
  2026-08-20 it is the mutable `0x3ba17675f0D319C89D70722f6eb07790DF0B254B` — `ISM-VALIDATORS.md`.)
- **TC → Solana** — **not supported**. ⚠️ **Watch the direction:** this is the **OPPOSITE**
  of §3. What **works and was proven is Solana→TC** (Solana as ORIGIN, §3). The
  **TC→Solana** direction (Solana as DESTINATION) is the one left out — because, to pay, the
  destination would have to prove who delivered, and Solana **does not record the executor**;
  proving would only be possible in the same tx (keeper), which was discarded. In short: **the
  Solana corridor works in the direction where it is the origin.** See `TRUSTLESS-RECEIPT.md` §G.

---

## 5. How YOU can verify it yourself (commands)

**TC — total and a specific id:**
```bash
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
terrad q wasm contract-state smart $VAULT '{"remote_config":{}}' --node $NODE          # total_remote_paid
terrad q wasm contract-state smart $VAULT '{"remote_claimed":{"message_id":"974a7e47…"}}' --node $NODE
```

**BSC — a specific id:**
```bash
cast call 0x34E06a7793877EC5251b1dC230aD7cD577d231f4 \
  "remoteClaimed(bytes32)(address,uint32,uint256,uint256)" 0x5920d3fb… \
  --rpc-url https://bsc-dataseed.bnbchain.org
```

**Solana — operator's PDA and balance:**
```bash
solana balance 8pz9ToVyJGcuF7enE4KERjQ9JG4My5vpm8XFvLwqer1j -u https://api.mainnet-beta.solana.com
```

**Decode any message (hex → fields):**
```bash
python3 -c 'import sys;b=bytes.fromhex(sys.argv[1]);print("origem",int.from_bytes(b[5:9],"big"),"destino",int.from_bytes(b[41:45],"big"),"recipient 0x"+b[45:77].hex(),"amount",int.from_bytes(b[109:141],"big"))' <HEX>
```

---

## 6. Summary of the 3 verified payments

| Corridor | msg_id | commission | receiving wallet | chain/tx |
|---|---|---|---|---|
| TC→BSC | `974a7e47…` | 33 LUNC | `terra1run…` | TC · `F4700EF4…` |
| BSC→TC | `5920d3fb…` | 2,259,538,750,000 wei BNB | `0x8f08…` | BSC · `remoteClaimed` |
| Solana→TC | `d5e2ab02…` + `d039daa1…` | 998,000 lamports SOL | `BirXd4Q…` | Solana · withdrawal `7mf9HE9C…` |

**Conclusion:** the payments **were made** and are recorded on-chain — they simply land on the
**origin chain** of each message, in its currency. Nothing was lost; it was a matter of
looking on the right chain.
