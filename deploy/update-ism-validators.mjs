// update-ism-validators — IGORFAKE warp: swaps the validator set of the synthetic
// ISMs (ETH, BSC, Solana) from [igorveras]/1 to the 4 validators with
// threshold 3 (3-of-4).
//
// ⚠️ 2026-08-20: the EVM part (--eth/--bsc/--fix-vault-bsc/--revert) was SUPERSEDED
// by storage-ism.mjs (definitive mutable ISMs 0x3ba17675…/0xF6b0cDD3…).
// From this script, only the SOLANA part (--sol) remains the rotation tool.
//
// ETH/BSC: the current ISMs (0xDe8e… / 0xa820…) are STATIC (validators in the
//   bytecode, no owner) — cannot be changed. The script creates the new ISM via
//   the official staticMessageIdMultisigIsmFactory factory (CREATE2 — deterministic,
//   idempotent address) and points the warp at it.
// Solana: the ISM 4MzF7… is the multisig-ism-message-id (mutable) — just send
//   SetValidatorsAndThreshold(132556) signed by the access-control owner.
//
//   usage:
//     DRY=1 node update-ism-validators.mjs --eth --bsc --sol   # only shows
//     node update-ism-validators.mjs --eth --bsc --sol         # executes
//   keys (env):
//     ETH_PRIVATE_KEY  → owner of ETH warp  (0xEF8181201Ce6C83120035Ffbcc11945E67Ba00ae)
//     BSC_PRIVATE_KEY  → owner of BSC warp  (0x8f085bAD1a15ee9ceeE58C83EFFFa72518975291)
//     SOLANA_KEYPAIR   → owner of Solana ISM (default: keypair BirXd4Q… in /home/lunc/keys)
//   RPCs: ETH_RPC / BSC_RPC / SOLANA_RPC (rpc.env)
import fs from "node:fs";
import { ethers } from "ethers";

const DRY = process.env.DRY === "1";
const want = (f) => process.argv.includes(f);
const log = (...a) => console.log(...a);

// new validators (igorveras · tcv · darksun · burnitall) — threshold 3-of-4
const VALIDATORS = [
  "0x71b2b8c36a0c76b74be92eb7915e26a69b3b03eb",
  "0x1afd3d07abd2aaa19a9f7993f334a926e253b90c",
  "0xe6bb040164a0ebbcb7e2d584f066c8b57dd74383",
  "0x5c374754892ebac52702475726b67f822efdfacc",
];
const THRESHOLD = 3;
const TC_DOMAIN = 132556;

// ---- EVM: factory.deploy(vals, 3) (idempotent) + warp.setInterchainSecurityModule ----
async function evm(name, rpc, warp, factory, keyEnv, legacy) {
  warp = ethers.getAddress(warp.toLowerCase()); factory = ethers.getAddress(factory.toLowerCase());
  const provider = new ethers.JsonRpcProvider(rpc);
  const FACTORY_ABI = [
    "function getAddress(address[],uint8) view returns (address)",
    "function deploy(address[],uint8) returns (address)",
  ];
  const WARP_ABI = [
    "function interchainSecurityModule() view returns (address)",
    "function setInterchainSecurityModule(address)",
    "function owner() view returns (address)",
  ];
  const ISM_ABI = ["function validatorsAndThreshold(bytes) view returns (address[],uint8)"];
  const fRO = new ethers.Contract(factory, FACTORY_ABI, provider);
  const wRO = new ethers.Contract(warp, WARP_ABI, provider);
  // ethers v6: contract.getAddress() is an internal method — use getFunction for the ABI function
  const ismAddr = await fRO.getFunction("getAddress(address[],uint8)").staticCall(VALIDATORS, THRESHOLD);
  const deployed = (await provider.getCode(ismAddr)) !== "0x";
  const current = await wRO.interchainSecurityModule();
  log(`${name}: new ISM (deterministic) = ${ismAddr} · ${deployed ? "ALREADY deployed" : "NOT yet deployed"}`);
  log(`${name}: warp ${warp} points today to ${current}`);
  if (current.toLowerCase() === ismAddr.toLowerCase()) { log(`${name}: ✓ nothing to do`); return; }
  if (DRY) return;
  const pk = process.env[keyEnv];
  if (!pk) { log(`${name}: ⚠ missing ${keyEnv} — skipping`); return; }
  const wallet = new ethers.Wallet(pk, provider);
  const owner = await wRO.owner();
  if (wallet.address.toLowerCase() !== owner.toLowerCase()) {
    log(`${name}: ⚠ key ${wallet.address} is not the warp owner ${owner} — skipping`); return;
  }
  const opts = legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {};
  if (!deployed) {
    const tx = await new ethers.Contract(factory, FACTORY_ABI, wallet).deploy(VALIDATORS, THRESHOLD, opts);
    log(`${name}: factory.deploy tx ${tx.hash} …`); await tx.wait();
  }
  const [vals, t] = await new ethers.Contract(ismAddr, ISM_ABI, provider).validatorsAndThreshold("0x");
  log(`${name}: new ISM verifies: ${vals.length} validators, threshold ${t}`);
  if (Number(t) !== THRESHOLD || vals.length !== VALIDATORS.length) { log(`${name}: ❌ ISM does not match — aborting`); return; }
  const tx2 = await new ethers.Contract(warp, WARP_ABI, wallet).setInterchainSecurityModule(ismAddr, opts);
  log(`${name}: setInterchainSecurityModule tx ${tx2.hash} …`); await tx2.wait();
  log(`${name}: ✓ warp now uses ${await wRO.interchainSecurityModule()}`);
}

// ---- Solana: SetValidatorsAndThreshold on the multisig-ism-message-id ----
async function sol() {
  const { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } = await import("@solana/web3.js");
  const RPC = process.env.SOLANA_RPC ?? "https://api.mainnet-beta.solana.com";
  const ISM = new PublicKey("4MzF7HCfxuwj4EFHqZSEpvkcZZvv1mF37DP4pDHwR5VQ");
  const sep = Buffer.from("-");
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
  const [acPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("multisig_ism_message_id"), sep, Buffer.from("access_control")], ISM);
  const [domPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("multisig_ism_message_id"), sep, u32(TC_DOMAIN), sep, Buffer.from("domain_data")], ISM);
  const conn = new Connection(RPC, "confirmed");
  // current state (domain_data: initialized u8 · bump u8 · Vec<H160> · threshold u8)
  const di = await conn.getAccountInfo(domPda);
  if (di) {
    const n = di.data.readUInt32LE(2);
    const cur = []; for (let i = 0; i < n; i++) cur.push("0x" + di.data.subarray(6 + i * 20, 26 + i * 20).toString("hex"));
    log(`SOL: today ${n} validator(s) [${cur.join(", ")}] threshold ${di.data[6 + n * 20]}`);
  }
  log(`SOL: new → ${VALIDATORS.length} validators, threshold ${THRESHOLD}`);
  if (DRY) return;
  const KEYPAIR = process.env.SOLANA_KEYPAIR ?? "/home/lunc/keys/solana-keypair-BirXd4QDxfq2vx9LGqgXXSgZrjT81rhoFGUbQRWDEf1j.json";
  const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(KEYPAIR, "utf8"))));
  log("SOL: signing as", kp.publicKey.toBase58());
  // data = discriminator [1×8] + borsh(enum): variant 1 + { domain u32 LE + Vec<H160> + threshold u8 }
  const data = Buffer.concat([
    Buffer.alloc(8, 1), Buffer.from([1]), u32(TC_DOMAIN),
    u32(VALIDATORS.length), ...VALIDATORS.map((v) => Buffer.from(v.slice(2), "hex")),
    Buffer.from([THRESHOLD]),
  ]);
  const ix = new TransactionInstruction({
    programId: ISM,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: acPda, isSigner: false, isWritable: false },
      { pubkey: domPda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  log("SOL: ✓ tx", sig);
  const after = await conn.getAccountInfo(domPda);
  const n = after.data.readUInt32LE(2);
  log(`SOL: now ${n} validators, threshold ${after.data[6 + n * 20]}`);
}

// ---- BSC: receipt vault also specifies ISM (setIsm, onlyOwner) ----
async function fixVaultBsc(target) {
  const provider = new ethers.JsonRpcProvider(process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org");
  const VAULT = ethers.getAddress("0x34e06a7793877ec5251b1dc230ad7cd577d231f4");
  const ABI = ["function interchainSecurityModule() view returns (address)", "function setIsm(address)", "function owner() view returns (address)"];
  const ro = new ethers.Contract(VAULT, ABI, provider);
  const cur = await ro.interchainSecurityModule();
  log(`BSC receipt vault ${VAULT}: ISM today ${cur} → target ${target}`);
  if (cur.toLowerCase() === target.toLowerCase()) { log("BSC vault: ✓ nothing to do"); return; }
  if (DRY) return;
  const pk = process.env.BSC_PRIVATE_KEY;
  if (!pk) { log("BSC vault: ⚠ missing BSC_PRIVATE_KEY — skipping"); return; }
  const wallet = new ethers.Wallet(pk, provider);
  const opts = { gasPrice: (await provider.getFeeData()).gasPrice };
  const tx = await new ethers.Contract(VAULT, ABI, wallet).setIsm(ethers.getAddress(target.toLowerCase()), opts);
  log(`BSC vault: setIsm tx ${tx.hash} …`); await tx.wait();
  log(`BSC vault: ✓ now uses ${await ro.interchainSecurityModule()}`);
}

// ---- revert: points the warps back to the old ISMs (1-of-1) ----
async function revert(name, rpc, warp, oldIsm, keyEnv, legacy) {
  const provider = new ethers.JsonRpcProvider(rpc);
  warp = ethers.getAddress(warp.toLowerCase()); oldIsm = ethers.getAddress(oldIsm.toLowerCase());
  const ABI = ["function interchainSecurityModule() view returns (address)", "function setInterchainSecurityModule(address)"];
  const cur = await new ethers.Contract(warp, ABI, provider).interchainSecurityModule();
  log(`${name} REVERT: warp points today to ${cur} → back to ${oldIsm}`);
  if (cur.toLowerCase() === oldIsm.toLowerCase()) { log(`${name}: ✓ already on the old one`); return; }
  if (DRY) return;
  const pk = process.env[keyEnv];
  if (!pk) { log(`${name}: ⚠ missing ${keyEnv} — skipping`); return; }
  const wallet = new ethers.Wallet(pk, provider);
  const opts = legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {};
  const tx = await new ethers.Contract(warp, ABI, wallet).setInterchainSecurityModule(oldIsm, opts);
  log(`${name}: tx ${tx.hash} …`); await tx.wait();
  log(`${name}: ✓ reverted`);
}

const NEW_ISM_BSC = "0xcA21D04eE1B1155d8548391770E1DFE3D9adc661";
if (want("--eth")) await evm("ETH", process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com",
  "0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5", "0xfA21D9628ADce86531854C2B7ef00F07394B0B69", "ETH_PRIVATE_KEY", false);
if (want("--bsc")) await evm("BSC", process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
  "0x3605D8946FC6F5A75d89d92173100F59743B5318", "0x4B1d8352E35e3BDE36dF5ED2e73C24E35c4a96b7", "BSC_PRIVATE_KEY", true);
if (want("--sol")) await sol();
if (want("--fix-vault-bsc")) await fixVaultBsc(NEW_ISM_BSC);
if (want("--revert")) {
  await revert("ETH", process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com",
    "0xA687a4C4CA49795999b36fDC8A18d1DDd63eDFB5", "0xDe8edEC7207e2dEf9D347Eaa1f6Ee50420bc070b", "ETH_PRIVATE_KEY", false);
  await revert("BSC", process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
    "0x3605D8946FC6F5A75d89d92173100F59743B5318", "0xa82087B8eea0394B1476f716B91c10531025Ef42", "BSC_PRIVATE_KEY", true);
}
log("done.");
