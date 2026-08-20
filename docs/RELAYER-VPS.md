# Relayer oficial da Hyperlane na VPS — versão, atualização e configuração

O transporte de TODAS as mensagens entre chains (transferências **e** recibos) é
feito pelo **relayer oficial da Hyperlane, sem nenhuma modificação de código** —
como manda a spec (§ "nenhuma linha do core Hyperlane é modificada"). Os nossos
agentes offline (`oracle-agent`, `claim-agent`, `epoch-reporter`) só fazem os
papéis que o relayer NÃO faz: preço/gás, emitir os claims (recibos), e o quórum
de época da Solana. O `deliver-receipts-tc.mjs` é apenas **rede de segurança**
(plano B, desligado por padrão — ver `RECIBO-TRUSTLESS.md`).

VPS `31.97.91.4` · serviço `hyperlane-relayer.service` · binário em
`/root/hyperlane/bin/relayer`.

## Versões

| | Versão | Commit | Origem | Data do binário |
|---|---|---|---|---|
| **Anterior** | build local | `906921a706a01b1d28a4936b06088f7cfa296851` | compilado localmente de `~/hyperlane-monorepo` | 2026-06-04 |
| **Atual** | **agents-v2.0.0** | `c117895a17dc5a932bc2007c15c53be26014e22d` | **imagem oficial** (não recompilado) | 2026-01-07 |

O binário anterior (109 MB) tinha dois defeitos próprios do build (não do nosso
código): rejeição de broadcast Cosmos no CheckTx (a MESMA tx via cosmjs passava)
e vazamento de file descriptors. Substituído pelo artefato **oficial** v2.0.0
(114 MB) — atualizar não é modificar código, é trocar um binário oficial velho
por um novo.

## Como foi feito (extração do binário oficial, sem recompilar)

O jeito canônico e reproduzível é pegar o binário **da imagem Docker oficial**
publicada pela Hyperlane, em vez de compilar (que na VPS esbarra na toolchain
rustc 1.84 vs edition2024 dos deps). Passos executados na VPS:

```bash
# 1. imagem oficial (gcr.io/abacus-labs-dev/hyperlane-agent)
docker pull gcr.io/abacus-labs-dev/hyperlane-agent:agents-v2.0.0
#    digest verificado:
#    sha256:e953983fee85fd01432f9e6a40e192cafc2c39db4a180aac34e55f8f624c964a

# 2. extrai o binário do relayer da imagem (não roda o container)
C=$(docker create gcr.io/abacus-labs-dev/hyperlane-agent:agents-v2.0.0)
docker cp $C:/app/relayer /root/hyperlane/bin/relayer-v2.0.0
docker rm $C
chmod +x /root/hyperlane/bin/relayer-v2.0.0

# 3. backup do binário antigo (rollback) + troca
cp /root/hyperlane/bin/relayer /root/hyperlane/bin/relayer-906921a7-backup
systemctl stop hyperlane-relayer
until ! ss -tlnp | grep -qE ':(9090|9091)\b'; do sleep 1; done   # porta liberar
cp /root/hyperlane/bin/relayer-v2.0.0 /root/hyperlane/bin/relayer
systemctl start hyperlane-relayer
```

**Rollback** (se preciso): `cp /root/hyperlane/bin/relayer-906921a7-backup
/root/hyperlane/bin/relayer && systemctl restart hyperlane-relayer`.

## Mudanças de CONFIGURAÇÃO (nenhuma no código)

1. **`metricsPort: 9091`** em `config/relayer.mainnet.json` — **causa dos 3
   panics `AddrInUse` / relayer zumbi**: na v2.0.0 o servidor do agente lê a chave
   `metricsPort` (default **9090**) e IGNORA o `--metrics 0.0.0.0:9091` legado do
   ExecStart; 9090 já é do **validator** → o servidor panica ao bindar e isso
   **mata os processors de mensagens** (o relayer indexava mas não entregava —
   era por isso que os recibos ficavam presos). Fixado apontando o relayer para
   9091 (validator segue no 9090).
2. **`relayApiEnabled: false` / `relayApiPort: 9092`** — desliga a API HTTP de
   controle do relayer (não usamos) e, se um dia ligar, fica em porta própria.
3. **`LimitNOFILE=1048576`** (drop-in `.../hyperlane-relayer.service.d/limits.conf`)
   — teto de file descriptors elevado (o binário antigo vazava fds; mantido como
   folga de segurança).
4. **RPCs** em `config/agent-config.mainnet.json`:
   - BSC: dataseeds oficiais primeiro (servem `eth_getLogs`) + publicnode/1rpc/drpc
     de reserva; `index.chunk = 50` (os públicos limitam getLogs a ≤50 blocos).
   - Solana: **Helius** (`mainnet.helius-rpc.com`, key própria) à frente do
     `api.mainnet-beta` público.
   - Terra Classic: hexxagon + publicnode + binodes.

> Regra operacional (chave compartilhada): a conta `terra1run9wz…` assina para o
> relayer, o claim-agent e scripts manuais. Depois de rodar qualquer script que
> assine com ela (migrate, igp-tariff, unenroll…), **reiniciar o relayer** para
> ressincronizar a sequence: `systemctl restart hyperlane-relayer`.

## Verificação pós-atualização

```bash
systemctl is-active hyperlane-relayer                       # active
journalctl -u hyperlane-relayer -n 200 | grep -c panicked   # 0
ss -tlnp | grep -E ':(9090|9091)'                           # 9091=relayer 9090=validator
journalctl -u hyperlane-relayer | grep 'starting up with version'  # c117895a…
```
Prova funcional: uma transferência nova deve ser entregue no destino sem
intervenção (o `delivered()`/`message_delivered` vira true e a comissão cai).
