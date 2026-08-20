// raise-bounds — alarga as faixas (bounds) dos governors nas 4 redes para
// vigente ÷10 · ×10 (antes: ÷3 · ×3). A fonte é sempre o valor VIGENTE lido do
// oracle/IGP de produção no momento da execução — nada de número fixo.
//
//   uso:
//     DRY=1 node raise-bounds.mjs --tc --bsc --eth --sol   # só mostra
//     node raise-bounds.mjs --tc --bsc                     # executa nas redes pedidas
//   chaves (env): TC_PRIVATE_KEY (owner do governor TC) · BSC_PRIVATE_KEY ·
//     ETH_PRIVATE_KEY (owner do governor ETH) · SOLANA_KEYPAIR (multisig do gov Solana)
//   RPCs: TC_RPC/BSC_RPC/ETH_RPC/SOLANA_RPC (rpc.env)
import fs from "node:fs";
import { ethers } from "ethers";
import { SigningCosmWasmClient, CosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { GasPrice } from "@cosmjs/stargate";
import { DirectSecp256k1Wallet } from "@cosmjs/proto-signing";

const DRY = process.env.DRY === "1";
const want = (f) => process.argv.includes(f);
const FACTOR = 10n;
const widen = (v) => ({ min: v / FACTOR > 0n ? v / FACTOR : 1n, max: v * FACTOR });
const log = (...a) => console.log(...a);

// ---- TC: set_bounds nos domínios 1 (ETH), 56 (BSC), 1399811149 (Solana) ----
async function tc() {
  const RPC = process.env.TC_RPC ?? "https://rpc.terra-classic.hexxagon.io";
  const GOV = "terra1z7jmlky2cmsd9aslm4uxrsase2yjwz8k9rlk00ga8s7pxgljczjq9sv4hj";
  const ORACLE = "terra1j8xzgzk7vds5uzrplmnln4vcz6f205t9atdyflypzrr43cd5eh7scwqj0d";
  const ro = await CosmWasmClient.connect(RPC);
  let client, sender;
  if (!DRY) {
    const hex = (process.env.TC_PRIVATE_KEY ?? "").replace(/^0x/, "");
    if (!hex) { log("TC: ⚠ falta TC_PRIVATE_KEY — pulando"); return; }
    const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(hex, "hex")), "terra");
    sender = (await wallet.getAccounts())[0].address;
    client = await SigningCosmWasmClient.connectWithSigner(RPC, wallet, { gasPrice: GasPrice.fromString("28.325uluna") });
    log("TC: assinando como", sender);
  }
  for (const dom of [1, 56, 1399811149]) {
    const cur = await ro.queryContractSmart(ORACLE, { oracle: { get_exchange_rate_and_gas_price: { dest_domain: dom } } });
    const r = widen(BigInt(cur.exchange_rate)), g = widen(BigInt(cur.gas_price));
    const bounds = {
      min_exchange_rate: r.min.toString(), max_exchange_rate: r.max.toString(),
      min_gas_price: g.min.toString(), max_gas_price: g.max.toString(),
    };
    log(`TC set_bounds dom ${dom}: vigente rate=${cur.exchange_rate} gas=${cur.gas_price} → rate [${bounds.min_exchange_rate}·${bounds.max_exchange_rate}] gas [${bounds.min_gas_price}·${bounds.max_gas_price}]`);
    if (DRY) continue;
    const res = await client.execute(sender, GOV, { set_bounds: { domain: dom, bounds } }, "auto");
    log(`  ✓ tx ${res.transactionHash}`);
  }
}

// ---- EVM: setBounds(132556) nos governors do BSC e do ETH ----
async function evm(name, rpc, gov, oracle, keyEnv, legacy) {
  const provider = new ethers.JsonRpcProvider(rpc);
  const ABI = [
    "function setBounds(uint32,(uint128,uint128,uint128,uint128,bool))",
    "function owner() view returns (address)",
  ];
  const oracleRO = new ethers.Contract(oracle, ["function getExchangeRateAndGasPrice(uint32) view returns (uint128,uint128)"], provider);
  const [rate, gas] = await oracleRO.getExchangeRateAndGasPrice(132556);
  const r = widen(rate), g = widen(gas);
  log(`${name} setBounds dom 132556: vigente rate=${rate} gas=${gas} → rate [${r.min}·${r.max}] gas [${g.min}·${g.max}]`);
  if (DRY) return;
  const pk = process.env[keyEnv];
  if (!pk) { log(`${name}: ⚠ falta ${keyEnv} — pulando`); return; }
  const wallet = new ethers.Wallet(pk, provider);
  const owner = await new ethers.Contract(gov, ABI, provider).owner();
  if (wallet.address.toLowerCase() !== owner.toLowerCase()) {
    log(`${name}: ⚠ chave ${wallet.address} não é o owner ${owner} — pulando`); return;
  }
  const c = new ethers.Contract(gov, ABI, wallet);
  const opts = legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {};
  const tx = await c.setBounds(132556, [r.min, r.max, g.min, g.max, true], opts);
  log(`  tx ${tx.hash} …`); await tx.wait();
  log("  ✓ confirmada");
}

// ---- Solana: SetDomainConfig(132556) no módulo governor do pod ----
async function sol() {
  const { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } = await import("@solana/web3.js");
  const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
  const POD = new PublicKey("2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj");
  const DOM = 132556;
  const sep = Buffer.from("-");
  const u8 = (n) => Buffer.from([Number(n)]);
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
  const u128 = (n) => { const b = Buffer.alloc(16); let v = BigInt(n); for (let i = 0; i < 16; i++) { b[i] = Number(v & 0xffn); v >>= 8n; } return b; };
  const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, POD)[0];
  const govConfig = pda([Buffer.from("gov"), sep, Buffer.from("config")]);
  const govDomain = pda([Buffer.from("gov"), sep, Buffer.from("domain"), sep, u32(DOM)]);
  const conn = new Connection(RPC, "confirmed");
  // vigente + decimals: do próprio DomainState (bump u8 · domain u32 · bounds 4×u128 · decimals u8 · last_rate/gas u128)
  const di = await conn.getAccountInfo(govDomain);
  if (!di) { log("SOL: ⚠ domain PDA não existe — rode solana-init.mjs antes"); return; }
  const rd = (off) => { let v = 0n; for (let i = 15; i >= 0; i--) v = (v << 8n) | BigInt(di.data[off + i]); return v; };
  const decimals = di.data[5 + 64];
  const lastRate = rd(5 + 64 + 1), lastGas = rd(5 + 64 + 1 + 16);
  const r = widen(lastRate), g = widen(lastGas);
  log(`SOL SetDomainConfig dom ${DOM}: vigente rate=${lastRate} gas=${lastGas} dec=${decimals} → rate [${r.min}·${r.max}] gas [${g.min}·${g.max}]`);
  if (DRY) return;
  const KEYPAIR = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";
  const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(KEYPAIR, "utf8"))));
  log("SOL: assinando como", kp.publicKey.toBase58());
  const ix = new TransactionInstruction({
    programId: POD,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: govConfig, isSigner: false, isWritable: false },
      { pubkey: govDomain, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([u8(1), u8(2), u32(DOM), u128(r.min), u128(r.max), u128(g.min), u128(g.max), u8(decimals)]),
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  log("  ✓ tx", sig);
}

if (want("--tc")) await tc();
if (want("--bsc")) await evm("BSC", process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
  "0x5CF7A3a7EA0c264c86a5faf248AfD5EDCd7913E5", "0x7dE950f8F0a037783989a6BE84B3620916552306", "BSC_PRIVATE_KEY", true);
if (want("--eth")) await evm("ETH", process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com",
  "0xa1803b366af48Cb16E0f44D24B4eb9f58643fEFA", "0x3987cCE8f08037EBF93Ef3a934753540A94196cE", "ETH_PRIVATE_KEY", false);
if (want("--sol")) await sol();
log("fim.");
