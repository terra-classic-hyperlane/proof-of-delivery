// Submission to the igp-oracle-governor (Solana): manual borsh of the instruction
//   SubmitPrice { domain: u32, token_exchange_rate: u128, gas_price: u128 }  (variant 1)
// REAL DEPLOY (18/08/2026): the governor lives inside the single `pod` program
// (2mQZcHYLFCXL1XnmmQdgCinYZW7yvuksqrdoHmNfZUFj) — the 1st byte of the instruction
// data selects the module (0=vault, 1=governor) and the rest is the instruction above.
// Accounts: [operator s w, config, domain w, round w, system, igp_program, igp w]
// Seeds same as the program's: ["gov","-","config"] · ["gov","-","domain","-",u32le]
//                              · ["gov","-","price","-",u32le,"-",u64le]
import fs from "node:fs";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";

const POD_MODULE_GOV = 1; // pod router: 0=vault (rrv), 1=governor
const SUBMIT_PRICE_VARIANT = 1;

function u32le(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(Number(n));
  return b;
}
function u64le(n) {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
}
function u128le(n) {
  const b = Buffer.alloc(16);
  let v = BigInt(n);
  for (let i = 0; i < 16; i++) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

export function submitPriceData(domain, rate, gasPrice) {
  return Buffer.concat([
    Buffer.from([POD_MODULE_GOV, SUBMIT_PRICE_VARIANT]),
    u32le(domain),
    u128le(rate),
    u128le(gasPrice),
  ]);
}

export function pdas(programId, domain, epoch) {
  const sep = Buffer.from("-");
  const gov = Buffer.from("gov");
  const [config] = PublicKey.findProgramAddressSync([gov, sep, Buffer.from("config")], programId);
  const [domainPda] = PublicKey.findProgramAddressSync(
    [gov, sep, Buffer.from("domain"), sep, u32le(domain)],
    programId,
  );
  const [round] = PublicKey.findProgramAddressSync(
    [gov, sep, Buffer.from("price"), sep, u32le(domain)],
    programId,
  );
  return { config, domainPda, round };
}

/** CURRENT value of the production IGP: scans the account by the LE domain and validates
 *  [domain u32][variant 0][rate u128][gas u128][decimals u8] (same parser
 *  as deploy/solana-init.mjs, tested against mainnet 18/08/2026). */
export async function readOracle(chain, domain) {
  const conn = new Connection(chain.rpc, "confirmed");
  const info = await conn.getAccountInfo(new PublicKey(chain.igpAccount));
  const d = info.data;
  const needle = u32le(domain);
  const rd = (p) => { let v = 0n; for (let i = 15; i >= 0; i--) v = (v << 8n) | BigInt(d[p + i]); return v; };
  let idx = -1;
  while ((idx = d.indexOf(needle, idx + 1)) !== -1) {
    if (idx + 38 > d.length) break;
    const rate = rd(idx + 5), gas = rd(idx + 21), dec = d[idx + 37];
    if (d[idx + 4] === 0 && rate > 0n && gas > 0n && dec >= 1 && dec <= 18) return { rate, gas };
  }
  throw new Error(`domain ${domain} not found in Igp ${chain.igpAccount}`);
}

export async function makeSolanaSubmitter(chain, epochDurationSecs) {
  // HEX key (privateKeyEnv, 32-byte ed25519 seed — Hyperlane relayer format)
  // OR path to a JSON keypair (keypairEnv)
  let keypair;
  const rawHex = chain.privateKeyEnv && process.env[chain.privateKeyEnv];
  if (rawHex) {
    keypair = Keypair.fromSeed(Uint8Array.from(Buffer.from(rawHex.replace(/^0x/, ""), "hex")));
  } else {
    const keypairPath = process.env[chain.keypairEnv];
    if (!keypairPath) throw new Error(`env ${chain.privateKeyEnv ?? chain.keypairEnv} missing`);
    keypair = Keypair.fromSecretKey(
      Uint8Array.from(JSON.parse(fs.readFileSync(keypairPath, "utf8"))),
    );
  }
  const connection = new Connection(chain.rpc, "confirmed");
  const governorId = new PublicKey(chain.governorProgram);
  const igpProgram = new PublicKey(chain.igpProgram);
  const igpAccount = new PublicKey(chain.igpAccount);

  return {
    sender: keypair.publicKey.toBase58(),
    async submit(domain, rate, gasPrice) {
      const epoch = BigInt(Math.floor(Date.now() / 1000 / epochDurationSecs));
      const { config, domainPda, round } = pdas(governorId, domain, epoch);
      const ix = new TransactionInstruction({
        programId: governorId,
        keys: [
          { pubkey: keypair.publicKey, isSigner: true, isWritable: true },
          { pubkey: config, isSigner: false, isWritable: false },
          { pubkey: domainPda, isSigner: false, isWritable: true },
          { pubkey: round, isSigner: false, isWritable: true },
          { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
          { pubkey: igpProgram, isSigner: false, isWritable: false },
          { pubkey: igpAccount, isSigner: false, isWritable: true },
        ],
        data: submitPriceData(domain, rate, gasPrice),
      });
      const tx = new Transaction().add(ix);
      const sig = await connection.sendTransaction(tx, [keypair]);
      await connection.confirmTransaction(sig, "confirmed");
      return sig;
    },
  };
}
