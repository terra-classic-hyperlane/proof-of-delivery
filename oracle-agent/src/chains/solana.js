// Submissão no igp-oracle-governor (Solana): borsh manual da instrução
//   SubmitPrice { domain: u32, token_exchange_rate: u128, gas_price: u128 }  (variante 1)
// Contas: [operator s w, config, domain w, round w, system, igp_program, igp w]
// Seeds iguais às do programa: ["gov","-","config"] · ["gov","-","domain","-",u32le]
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
    Buffer.from([SUBMIT_PRICE_VARIANT]),
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
    [gov, sep, Buffer.from("price"), sep, u32le(domain), sep, u64le(epoch)],
    programId,
  );
  return { config, domainPda, round };
}

export async function makeSolanaSubmitter(chain, epochDurationSecs) {
  const keypairPath = process.env[chain.keypairEnv];
  if (!keypairPath) throw new Error(`env ${chain.keypairEnv} ausente`);
  const keypair = Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(keypairPath, "utf8"))),
  );
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
