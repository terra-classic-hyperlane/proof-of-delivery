# Guia — apontar o validador `tcv` para a MAINNET do Terra Classic

**Para:** operador do validador `tcv` (`0x1afd3d07abd2aaa19a9f7993f334a926e253b90c`).

**Situação atual:** o teu validador está **online e assinando**, mas apontado para a
**testnet** (`mailbox_domain: 1325` no teu `announcement.json`). Por isso as tuas
assinaturas **não valem** para os recibos de mainnet, e no `validatorAnnounce` da
mainnet o teu endereço aparece **vazio**. O ISM 3-de-4 da mainnet hoje opera com
só 3 validadores efetivos — o teu entra como o 4º (a folga) assim que apontar
para a mainnet e anunciar.

**O que muda:** origem `terraclassic` **mainnet** (domain **132556**, chain-id
`columbus-5`), um bucket S3 dedicado à mainnet, e um signer Cosmos com um pouco de
LUNC (o validador **anuncia sozinho** no primeiro start, se tiver gás).

---

## 1. Bucket S3 dedicado à MAINNET

Não misture testnet e mainnet no mesmo bucket. Crie um novo (ou um prefixo):
- ex.: `hyperlane-validator-signatures-tcv-mainnet` (região à sua escolha, ex.: `eu-central-1`)
- políticas: **leitura pública** dos objetos (o relayer/ISM lê os checkpoints via HTTPS),
  escrita só pela sua conta. (mesmo esquema do bucket testnet que já funciona.)

## 2. Config do validador (mainnet)

Dois arquivos, como os validadores que já funcionam.

**`agent-config.mainnet.json`** — a chain `terraclassic` da mainnet (endereços
oficiais, conferidos on-chain):

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

> Em bech32 (só p/ conferência): mailbox `terra1fwg35n5esjgny7d8pxnz8usjpwsvpguk0txsy6cnqxy58x9fdlksjpx3p9`,
> merkleTreeHook `terra183lq6yqp8km3p34cxgk6k3u78uy4plqahey6rne7n9gy98delr9qyp0n2p`,
> validatorAnnounce `terra1gtnmdevekgxpvzej3wfy20e2n335gm3muwj6geduxxa86j3x70cq00asmy`.

**`validator.terraclassic.json`** — o teu bucket de mainnet + as tuas chaves
(NÃO comitar; `chmod 600`):

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
    "key": "0xSUA_CHAVE_DE_ASSINATURA_DO_VALIDADOR"   // a mesma do 0x1afd… (assina os checkpoints)
  },
  "chains": {
    "terraclassic": {
      "signer": {
        "type": "cosmosKey",
        "key": "0xSUA_CHAVE_HEX_COSMOS",              // carteira terra1… com LUNC p/ o announce
        "prefix": "terra"
      }
    }
  }
}
```

- A **chave do `validator`** (assina os checkpoints) é a que gera o endereço
  `0x1afd…` — **mantenha a mesma** (senão vira outro validador e não conta no ISM).
- A **chave do `signer` Cosmos** é uma carteira `terra1…` que paga o gás do
  **announce** (uma tx única). **Financie com uns 20-50 LUNC.**

## 3. Rodar

Igual ao validador de referência (systemd), mas com o `--originChainName terraclassic`
apontando para a config de **mainnet**:

```bash
validator \
  --db /tmp/hyp/validator-mainnet/db \
  --originChainName terraclassic \
  --checkpointSyncer.type s3 \
  --metrics 0.0.0.0:9090 \
  --config /caminho/agent-config.mainnet.json,/caminho/validator.terraclassic.json
```

No **primeiro start**, o validador **anuncia sozinho** o storage location no
`validatorAnnounce` da mainnet (por isso o signer Cosmos precisa de LUNC). Depois
começa a assinar os checkpoints da árvore de mainnet e a publicá-los no bucket.

## 4. Verificação (você e o time)

Depois de rodar, confira os 3 sinais:

```bash
# (a) o announcement agora é de MAINNET (domain 132556, não 1325):
curl -s https://<seu-bucket-mainnet>.s3.<região>.amazonaws.com/announcement.json | grep mailbox_domain

# (b) o índice acompanha a árvore de mainnet (hoje ~31; deve subir junto):
curl -s https://<seu-bucket-mainnet>.s3.<região>.amazonaws.com/checkpoint_latest_index.json

# (c) o announce aparece ON-CHAIN (não mais vazio):
#     consulte get_announce_storage_locations no validatorAnnounce de mainnet
```

No **painel de operação** (`http://localhost:8787`), o `tcv` deve sair de
**"rede errada (dom 1325)"** para **`ok`**, e o 3-de-4 passa a mostrar **4/4**.

## Checklist rápido

- [ ] Bucket S3 de mainnet criado (leitura pública)
- [ ] `agent-config.mainnet.json` com a chain `terraclassic` (domain 132556)
- [ ] `validator.terraclassic.json` com o bucket de mainnet + chave de assinatura `0x1afd…` + signer Cosmos
- [ ] Signer Cosmos financiado com LUNC (p/ o announce)
- [ ] Validador rodando com `--originChainName terraclassic` (config de mainnet)
- [ ] `announcement.json` → `mailbox_domain: 132556`
- [ ] `checkpoint_latest_index` acompanhando o tip da mainnet
- [ ] announce visível on-chain
