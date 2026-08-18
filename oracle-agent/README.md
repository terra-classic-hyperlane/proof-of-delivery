# oracle-agent

O lado **off-chain do operador de relayer**: observa o preço dos tokens nativos
(CoinGecko) e o gás dos domínios remotos, e submete `SubmitPrice` ao **governor
de cada chain** da ponte — Terra Classic (CosmWasm), BSC e Ethereum (EVM) e
Solana. É genérico: qualquer chain nova entra por configuração.

O agente **não tem poder**: quem aplica é o governor on-chain, com quórum entre
os operadores, **mediana** (menor dos centrais), **faixa** definida pela
governança/multisig e **delta máximo** por época. Um agente comprometido, no
pior caso, submete um número que os outros operadores não confirmam.

## Como funciona

1. A cada rodada (padrão 1h): busca os preços USD de todos os tokens de uma vez;
2. Para cada chain local habilitada, para cada domínio remoto:
   `token_exchange_rate = preço(remoto)/preço(local) × scale` — **scale por VM**:
   `1e10` (CosmWasm/EVM) e `1e19` (Solana), como a spec §08 exige;
3. Gas price do remoto: `eth_gasPrice` via RPC (EVM) ou valor fixo configurado;
4. Submete ao governor da chain local (CosmWasm execute · EVM `submitPrice` ·
   Solana instrução borsh com as PDAs derivadas).

## Uso

```bash
npm install
cp config.example.json config.json   # preencha governors/RPCs/domínios

# ver o que seria submetido, sem assinar nada:
npm run dry-run

# uma rodada real (bom para cron):
TC_MNEMONIC="..." EVM_PRIVATE_KEY="0x..." SOLANA_KEYPAIR_PATH=~/keypair.json npm run once

# loop contínuo:
npm start
```

Chaves só por variável de ambiente (`mnemonicEnv` / `privateKeyEnv` /
`keypairEnv` no config dizem QUAL env ler) — nada de segredo em arquivo de
configuração.

## Testes

```bash
npm test    # node:test — matemática do exchange_rate e escalas por VM
```

## Operação (vários operadores)

Cada operador roda o **seu** agente com a **sua** chave. Não há coordenação:
todos observam o mercado de forma independente e o governor converge pela
mediana. Recomenda-se cron a cada época (6h) com pequeno jitter por operador.
