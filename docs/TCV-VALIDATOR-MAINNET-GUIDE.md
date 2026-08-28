# Guide — pointing the `tcv` validator at the Terra Classic MAINNET

**For:** the operator of the `tcv` validator (`0x1afd3d07abd2aaa19a9f7993f334a926e253b90c`).

**Current situation:** your validator is **online and signing**, but pointed at
the **testnet** (`mailbox_domain: 1325` in your `announcement.json`). Because of
that your signatures **are not valid** for the mainnet receipts, and in the
mainnet `validatorAnnounce` your address shows up **empty**. The mainnet 3-of-4
ISM currently operates with only 3 effective validators — yours comes in as the
4th (the slack) as soon as you point at the mainnet and announce.

**What changes:** origin `terraclassic` **mainnet** (domain **132556**, chain-id
`columbus-5`), an S3 bucket dedicated to mainnet, and a Cosmos signer with a bit
of LUNC (the validator **announces on its own** on the first start, if it has
gas).

---

## 1. S3 bucket dedicated to MAINNET

Do not mix testnet and mainnet in the same bucket. Create a new one (or a prefix):
- e.g.: `hyperlane-validator-signatures-tcv-mainnet` (region of your choice, e.g.: `eu-central-1`)
- policies: **public read** of the objects (the relayer/ISM reads the checkpoints via HTTPS),
  write only by your account. (same scheme as the testnet bucket that already works.)

## 2. Validator config (mainnet)

Two files, like the validators that already work.

**`agent-config.mainnet.json`** — the mainnet `terraclassic` chain (official
addresses, checked on-chain):

```json
{
  "chains": {
    "terraclassic": {
      "name": "terraclassic",
      "chainId": "columbus-5",
      "domainId": 132556,
      "protocol": "cosmos",
      "bech32Prefix": "terra",
      "gasPrice": { "amount": "28325", "denom": "uluna" },
      "mailbox":            "0x4b911a4e9984913279a709a623f2120ba0c0a3967acd026b1301894398a96fed",
      "merkleTreeHook":     "0x3c7e0d10013db710c6b8322dab479e3f0950fc1dbe49a1cf3e9950429db9f8ca",
      "validatorAnnounce":  "0x42e7b6e599b20c160b328b92453f2a9c63446e3be3a5a465bc31ba7d4a26f3f0",
      "interchainGasPaymaster": "0x5f793ba34a28e104c505896601bef42d414dc20313654fd8cab911b36efe522e",
      "index": { "from": 28905457 },
      "rpcUrls": [
        { "http": "https://rpc.terra-classic.hexxagon.io" },
        { "http": "https://terra-classic-rpc.publicnode.com:443" }
      ]
    }
  }
}
```

> In bech32 (just for cross-checking): mailbox `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9`,
> merkleTreeHook `terra183lq6yqp8km3p34cxgk6k3u78uy4plqahey6rne7n9gy98delr9qyp0n2p`,
> validatorAnnounce `terra1gtnmdevekgxpvzej3wfy20e2n335gm3muwj6geduxxa86j3x70cq00asmy`.

**`validator.terraclassic.json`** — your mainnet bucket + your keys
(do NOT commit; `chmod 600`):

```json
{
  "db": "/tmp/hyp/validator-mainnet/db",
  "originChainName": "terraclassic",
  "checkpointSyncer": {
    "type": "s3",
    "bucket": "hyperlane-validator-signatures-tcv-mainnet",
    "region": "eu-central-1"
  },
  "validator": {
    "type": "hexKey",
    "key": "0xYOUR_KEY_DE_ASSINATURA_DO_VALIDADOR"   // the same as 0x1afd… (signs the checkpoints)
  },
  "chains": {
    "terraclassic": {
      "signer": {
        "type": "cosmosKey",
        "key": "0xYOUR_KEY_HEX_COSMOS",              // terra1… wallet with LUNC for the announce
        "prefix": "terra"
      }
    }
  }
}
```

- The **`validator` key** (signs the checkpoints) is the one that generates the
  address `0x1afd…` — **keep it the same** (otherwise it becomes another validator
  and does not count in the ISM).
- The **Cosmos `signer` key** is a `terra1…` wallet that pays the gas of the
  **announce** (a single tx). **Fund it with about 20-50 LUNC.**

## 3. Run

Same as the reference validator (systemd), but with `--originChainName terraclassic`
pointing at the **mainnet** config:

```bash
validator \
  --db /tmp/hyp/validator-mainnet/db \
  --originChainName terraclassic \
  --checkpointSyncer.type s3 \
  --metrics 0.0.0.0:9090 \
  --config /path/to/agent-config.mainnet.json,/path/to/validator.terraclassic.json
```

On the **first start**, the validator **announces on its own** the storage
location in the mainnet `validatorAnnounce` (that is why the Cosmos signer needs
LUNC). Then it starts signing the checkpoints of the mainnet tree and publishing
them to the bucket.

## 4. Verification (you and the team)

After running, check the 3 signals:

```bash
# (a) the announcement is now MAINNET (domain 132556, not 1325):
curl -s https://<seu-bucket-mainnet>.s3.<região>.amazonaws.com/announcement.json | grep mailbox_domain

# (b) the index follows the mainnet tree (today ~31; it should rise along with it):
curl -s https://<seu-bucket-mainnet>.s3.<região>.amazonaws.com/checkpoint_latest_index.json

# (c) the announce shows up ON-CHAIN (no longer empty):
#     query get_announce_storage_locations on the mainnet validatorAnnounce
```

On the **operations panel** (`http://localhost:8787`), `tcv` should go from
**"wrong network (dom 1325)"** to **`ok`**, and the 3-of-4 starts showing **4/4**.

## Quick checklist

- [ ] Mainnet S3 bucket created (public read)
- [ ] `agent-config.mainnet.json` with the `terraclassic` chain (domain 132556)
- [ ] `validator.terraclassic.json` with the mainnet bucket + signing key `0x1afd…` + Cosmos signer
- [ ] Cosmos signer funded with LUNC (for the announce)
- [ ] Validator running with `--originChainName terraclassic` (mainnet config)
- [ ] `announcement.json` → `mailbox_domain: 132556`
- [ ] `checkpoint_latest_index` following the mainnet tip
- [ ] announce visible on-chain
