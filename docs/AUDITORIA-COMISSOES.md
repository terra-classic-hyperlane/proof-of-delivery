# Auditoria de Comissões — casos de teste on-chain (todos os corredores)

> Verificação real, on-chain, de **quem recebeu a comissão, quanto, em qual tx**, com
> **todas as mensagens em hex e decodificadas**. Feito para análise/auditoria.
> Data da coleta: 2026-08-19.

## 0. LEIA ISTO PRIMEIRO — por que você "não encontrou" o pagamento

A comissão de uma mensagem **X → Y** é paga **na chain de ORIGEM (X), na moeda de X**,
para o endereço do operador **naquela chain** — **não** na chain de destino.

| Você entregou uma msg… | A comissão cai em… | Na moeda… | Provado abaixo |
|---|---|---|---|
| **TC → BSC** (entregou no BSC) | **TC** | **LUNC** | §1 |
| **BSC → TC** (entregou no TC) | **BSC** | **BNB** | §2 |
| **Solana → TC** (entregou no TC) | **Solana** | **SOL** | §3 |
| **TC → ETH / ETH → TC** | — | — | §4 (vault ETH não deployado) |
| **TC → Solana** | — | — | §4 (não suportado sem keeper) |

Se você entregou uma mensagem **TC→BSC** e foi procurar o pagamento **no BSC**, não ia
achar: ele está **no TC, em LUNC**. E vice-versa. É a causa mais provável da confusão.

### Totais on-chain (confirmam que houve pagamento)
- **TC** `total_remote_paid` = **165.000.000 uluna = 165 LUNC** (query `remote_config`).
- **BSC** vault `0x34E06a77…` — pagamentos em BNB registrados em `remoteClaimed` (§2).
- **Solana** — creditado na PDA do operador e sacado (§3).

---

## 1. Corredor TC → BSC  (origem TC paga **LUNC** no TC) ✅

**Mensagem original** (transferência IGORFAKE saindo do TC):
- `message_id`: `974a7e472521a652b55758550f3786d6f34cf3a01c9b1652ada4256b5c56ea8d`
- **hex**:
  ```
  0300000012000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000380000000000000000000000003605d8946fc6f5a75d89d92173100f59743b5318000000000000000000000000867f9ce9f0d7218b016351cb6122406e6d247a5e00000000000000000000000000000000000000000000000000000000002625a0
  ```
- **decodificado**:
  | campo | valor |
  |---|---|
  | versão | 3 |
  | nonce | 18 |
  | origem | `132556` (Terra Classic) |
  | destino | `56` (BSC) |
  | sender (warp TC) | `0x70fd6184…d4a2da859` |
  | recipient (warp BSC) | `0x…3605d8946fc6f5a75d89d92173100f59743b5318` |
  | **carteira que recebe o token** | `0x867f9ce9f0d7218b016351cb6122406e6d247a5e` (BSC) |
  | **valor transferido** | `2.500.000` unidades IGORFAKE |

**Entrega**: operador índice 0 (`terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp`) entregou no BSC.

**Recibo → pagamento no TC** (tx `F4700EF49F734DEE8171C3BB93AEAC8EB1F0157B781BB6879CBAE1F381A4B126`, evento `handle_receipt`, origem do recibo = 56):

| **COMISSÃO** | |
|---|---|
| Recebida por | `terra1run9wz09uhh6pu7ggcwwetrgye4wu7wn26mawp` (carteira TC do operador) |
| Valor | **33.000.000 uluna = 33 LUNC** |
| Registro on-chain | `remote_claimed[974a7e47…]` → `{claimed:true, executor:terra1run…, domain:56, amount:33000000, block:30019234}` |

> Total TC = 165 LUNC ⇒ há **~5 pagamentos** desse tipo (33 LUNC cada). O RPC público
> só indexa uma janela recente (recuperamos 1 tx: `F4700EF4`); os demais confirmam-se
> pelo total e por `remote_claimed[<id>]` de cada id (ver §5).

---

## 2. Corredor BSC → TC  (origem BSC paga **BNB** no BSC) ✅

**Mensagem original** (IGORFAKE saindo do BSC):
- `message_id`: `5920d3fbf1d68e4cd3a5e0e4bb834ec83fc40d8e0f8ea2ef3530b3efe038ca84`
- **hex**:
  ```
  03000649c4000000380000000000000000000000003605d8946fc6f5a75d89d92173100f59743b5318000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000000000000000000000fedd34151143a14c158feb8cdeced2febaa0c1370000000000000000000000000000000000000000000000000000000000c65d40
  ```
- **decodificado**:
  | campo | valor |
  |---|---|
  | versão | 3 |
  | nonce | 412100 |
  | origem | `56` (BSC) |
  | destino | `132556` (Terra Classic) |
  | sender (warp BSC) | `0x…3605d8946fc6f5a75d89d92173100f59743b5318` |
  | recipient (warp TC) | `0x70fd6184…d4a2da859` |
  | **carteira que recebe o token** | `0xfedd34151143a14c158feb8cdeced2febaa0c137` = `terra1lmwng9g3gws5c9v0awxdankjl6a2psfhm8pc8z` |
  | **valor transferido** | `13.000.000` unidades IGORFAKE |

**Entrega**: operador índice 0 entregou no TC (tx `EA39249CBC0E11434DD70A575F015BD9DF38AE02F22D6210E42E055B31178370`).

**Recibo → pagamento no BSC** (vault `0x34E06a7793877EC5251b1dC230aD7cD577d231f4`):

| **COMISSÃO** | |
|---|---|
| Recebida por | `0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291` (carteira BSC do operador) |
| Valor | **2.259.538.750.000 wei = 0,00000225953875 BNB** |
| Registro on-chain | `remoteClaimed(5920d3fb…)` → `(0x8f08…, 132556, 2259538750000, block 116873093)` |

---

## 3. Corredor Solana → TC  (origem Solana paga **SOL** na Solana) ✅ *(provado nesta rodada)*

**Mensagens originais** (IGORFAKE saindo da Solana — duas, num único recibo/batch):

**Msg A** — `message_id` `d5e2ab02bef59776d6c6dc43e1e566e15890c986f36e4fdebf2c5af37cdacc4f`
```
030005cc08536f6c4dc6de5b1fd8d285c06fa3967440530edfec35e907464599e3b485c5f273437f95000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da8590000000000000000000000003fc7ee49a59c1041d4a58bc21ef657eb443c8bbb00000000000000000000000000000000000000000000000000000000005b8d80
```
| campo | valor |
|---|---|
| origem → destino | `1399811149` (Solana) → `132556` (TC) |
| sender (warp Solana) | `0x536f6c4d…273437f95` |
| **carteira que recebe** | `0x3fc7ee49…443c8bbb` = `terra18lr7ujd9nsgyr49930ppaajhadzrezam70j39k` |
| **valor transferido** | `6.000.000` unidades IGORFAKE |

**Msg B** — `message_id` `d039daa1c75d5b558906fef6d790b13dc94a8b39e58e1e7f219b3967a28c4f04`
```
030005cc01536f6c4dc6de5b1fd8d285c06fa3967440530edfec35e907464599e3b485c5f273437f95000205cc70fd6184ff0a5ad088c9b199bba6666bf4cb0a35cf92f5d94c27791d4a2da859000000000000000000000000fedd34151143a14c158feb8cdeced2febaa0c13700000000000000000000000000000000000000000000000000000000002dc6c0
```
| campo | valor |
|---|---|
| origem → destino | `1399811149` (Solana) → `132556` (TC) |
| **carteira que recebe** | `0xfedd3415…baa0c137` = `terra1lmwng9g3gws5c9v0awxdankjl6a2psfhm8pc8z` |
| **valor transferido** | `3.000.000` unidades IGORFAKE |

**Entrega**: operador índice 0 entregou no TC (txs `6B6BCA15…` e `4126C514…`).

**Recibo** (send_receipt no TC, tx `FD720251DAA642AC7EE65C36BC7AFB977BD4C9729007D82204AA9AE23CBF67A3`) →
recibo `5f67d0f7eec906e72bf724f1333b1657b6c924773ee88a6e33a62706a421158a` entregue no `pod`
(PDA `ProcessedMessage` `pFtaCoYr9UQaMLjVwD5SGp8KZeVDXnH8vqYxhDzmgZ6`).

| **COMISSÃO** | |
|---|---|
| Creditada na PDA | `operator_sol(0)` = `8pz9ToVyJGcuF7enE4KERjQ9JG4My5vpm8XFvLwqer1j` |
| Valor | **998.000 lamports = 0,000998 SOL** (2 × 499.000) |
| **Sacada para** | `BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j` (carteira Solana do operador) |
| Tx do saque | `7mf9HE9Ck5fYqRg2XnLt9VoArFw3HBYUjhsZmsY2GLh5yk79mnDNy8XDaqsCdvQ18NiXwQFT8XYXLEGcMqUecU5` |

> Na Solana o pagamento é em **2 passos**: o `handle` credita a PDA do operador e o
> operador **saca** (`WithdrawOperatorSol`). Se olhar só a carteira, confira **antes**
> a PDA `operator_sol(index)` e o tx de saque.

---

## 4. Corredores SEM comissão (esperado)

- **TC ↔ ETH** — o vault de recibo do ETH **ainda não foi deployado** (aguardando gás
  baixo). Sem vault, não há recibo nem pagamento. **Nenhuma comissão de ETH existe** —
  não é bug, é etapa pendente. (ISM do warp ETH: `0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b`.)
- **TC → Solana** — **não suportado**: a Solana não grava quem entregou, exigiria
  relayer customizado (keeper), que foi descartado. Só **Solana→TC** paga. Ver
  `RECIBO-TRUSTLESS.md` §G.

---

## 5. Como VOCÊ mesmo verificar (comandos)

**TC — total e um id específico:**
```bash
NODE=https://rpc.terra-classic.hexxagon.io:443
VAULT=terra1gqkrh2va5mqdrlp90ez6lc2hgagxqju6fc7md4kldlz8lap9w4usduzc2q
terrad q wasm contract-state smart $VAULT '{"remote_config":{}}' --node $NODE          # total_remote_paid
terrad q wasm contract-state smart $VAULT '{"remote_claimed":{"message_id":"974a7e47…"}}' --node $NODE
```

**BSC — um id específico:**
```bash
cast call 0x34E06a7793877EC5251b1dC230aD7cD577d231f4 \
  "remoteClaimed(bytes32)(address,uint32,uint256,uint256)" 0x5920d3fb… \
  --rpc-url https://bsc-dataseed.bnbchain.org
```

**Solana — PDA do operador e saldo:**
```bash
solana balance 8pz9ToVyJGcuF7enE4KERjQ9JG4My5vpm8XFvLwqer1j -u https://api.mainnet-beta.solana.com
```

**Decodificar qualquer mensagem (hex → campos):**
```bash
python3 -c 'import sys;b=bytes.fromhex(sys.argv[1]);print("origem",int.from_bytes(b[5:9],"big"),"destino",int.from_bytes(b[41:45],"big"),"recipient 0x"+b[45:77].hex(),"amount",int.from_bytes(b[109:141],"big"))' <HEX>
```

---

## 6. Resumo dos 3 pagamentos verificados

| Corredor | msg_id | comissão | carteira que recebeu | chain/tx |
|---|---|---|---|---|
| TC→BSC | `974a7e47…` | 33 LUNC | `terra1run…` | TC · `F4700EF4…` |
| BSC→TC | `5920d3fb…` | 2.259.538.750.000 wei BNB | `0x8f08…` | BSC · `remoteClaimed` |
| Solana→TC | `d5e2ab02…` + `d039daa1…` | 998.000 lamports SOL | `BirXd4Q…` | Solana · saque `7mf9HE9C…` |

**Conclusão:** os pagamentos **foram feitos** e estão registrados on-chain — só caem na
**chain de origem** de cada mensagem, na moeda dela. Nada foi perdido; era questão de
olhar na chain certa.
