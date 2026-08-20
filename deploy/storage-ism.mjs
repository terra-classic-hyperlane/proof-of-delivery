// storage-ism — ISM MUTÁVEL (StorageMessageIdMultisigIsm) p/ o warp IGORFAKE em
// ETH e BSC. Resolve de vez o problema do ISM estático: implanta UMA vez e o
// endereço nunca mais muda — entrada/saída de validador vira só uma tx do owner
// (setValidatorsAndThreshold), sem tocar em warp/vault de ninguém.
// (Na Solana o ISM já é assim — programa 4MzF7…, editado no lugar.)
//
// Fases (por chain):
//   --deploy : 1) implanta a StorageMessageIdMultisigIsmFactory (artifact compilado
//                 do monorepo); 2) factory.deploy(VALIDATORS, THRESHOLD) → cria o
//                 ISM mutável (proxy, owner = quem assina); 3) warp.setInterchain-
//                 SecurityModule(ism); 4) BSC: vault de recibo.setIsm(ism).
//                 Endereços salvos em storage-ism.state (JSON).
//   --set    : setValidatorsAndThreshold(VALIDATORS, THRESHOLD) no ISM salvo —
//                 é ISSO que se roda nas próximas rotações de validador.
//   --show   : mostra o estado on-chain (warp→ISM, validadores, owner).
//
//   uso:
//     DRY=1 node storage-ism.mjs --deploy --eth --bsc
//     ETH_PRIVATE_KEY=0x… BSC_PRIVATE_KEY=0x… node storage-ism.mjs --deploy --eth --bsc
//     node storage-ism.mjs --set --eth --bsc      # rotações futuras (edite VALIDATORS)
//     node storage-ism.mjs --show --eth --bsc
import fs from "node:fs";
import { ethers } from "ethers";

const DRY = process.env.DRY === "1";
const want = (f) => process.argv.includes(f);
const log = (...a) => console.log(...a);

// conjunto vigente — edite AQUI nas próximas rotações e rode --set
const VALIDATORS = [
  "0x71b2b8c36a0c76b74be92eb7915e26a69b3b03eb", // igorveras
  "0x1afd3d07abd2aaa19a9f7993f334a926e253b90c", // tcv
  "0xe6bb040164a0ebbcb7e2d584f066c8b57dd74383", // darksun
  "0x5c374754892ebac52702475726b67f822efdfacc", // burnitall
];
const THRESHOLD = 3;

const ARTIFACT = "/home/lunc/smart-hyperlane-monorepo/solidity/out/StorageMultisigIsm.sol/StorageMessageIdMultisigIsmFactory.json";
const STATE_FILE = new URL("./storage-ism.state", import.meta.url).pathname;
const state = fs.existsSync(STATE_FILE) ? JSON.parse(fs.readFileSync(STATE_FILE, "utf8")) : {};
const save = () => fs.writeFileSync(STATE_FILE, JSON.stringify(state, null, 1));

const CHAINS = {
  eth: {
    name: "ETH", keyEnv: "ETH_PRIVATE_KEY", legacy: false,
    rpc: process.env.ETH_RPC ?? "https://ethereum-rpc.publicnode.com",
    warp: "0xa687a4c4ca49795999b36fdc8a18d1ddd63edfb5",
    vault: null, // vault de recibo do ETH ainda não deployado — quando existir, setIsm nele
  },
  bsc: {
    name: "BSC", keyEnv: "BSC_PRIVATE_KEY", legacy: true,
    rpc: process.env.BSC_RPC ?? "https://bsc-dataseed.bnbchain.org",
    warp: "0x3605d8946fc6f5a75d89d92173100f59743b5318",
    vault: "0x34e06a7793877ec5251b1dc230ad7cd577d231f4",
  },
};

const ISM_ABI = [
  "function validatorsAndThreshold(bytes) view returns (address[],uint8)",
  "function setValidatorsAndThreshold(address[],uint8)",
  "function owner() view returns (address)",
];
const WARP_ABI = [
  "function interchainSecurityModule() view returns (address)",
  "function setInterchainSecurityModule(address)",
  "function owner() view returns (address)",
];
const VAULT_ABI = ["function interchainSecurityModule() view returns (address)", "function setIsm(address)"];

function signer(c, provider) {
  const pk = process.env[c.keyEnv];
  if (!pk) { log(`${c.name}: ⚠ falta ${c.keyEnv}`); return null; }
  return new ethers.Wallet(pk, provider);
}
const optsFor = async (c, provider) => (c.legacy ? { gasPrice: (await provider.getFeeData()).gasPrice } : {});

async function deploy(c) {
  const provider = new ethers.JsonRpcProvider(c.rpc);
  const warp = ethers.getAddress(c.warp);
  const st = (state[c.name] ??= {});
  log(`\n== ${c.name} — deploy do ISM mutável ==`);
  if (DRY) { log(`${c.name}: DRY — implantaria factory + ISM(${VALIDATORS.length} validadores, t=${THRESHOLD}) e apontaria warp ${warp}${c.vault ? " + vault" : ""}`); return; }
  const w = signer(c, provider); if (!w) return;
  const wOwner = await new ethers.Contract(warp, WARP_ABI, provider).owner();
  if (w.address.toLowerCase() !== wOwner.toLowerCase()) { log(`${c.name}: ⚠ chave ${w.address} não é o owner do warp ${wOwner} — abortando`); return; }
  const opts = await optsFor(c, provider);

  // 1) factory
  if (!st.factory) {
    const art = JSON.parse(fs.readFileSync(ARTIFACT, "utf8"));
    const cf = new ethers.ContractFactory(art.abi, art.bytecode.object, w);
    const fc = await cf.deploy(opts);
    log(`${c.name}: factory tx ${fc.deploymentTransaction().hash} …`);
    await fc.waitForDeployment();
    st.factory = await fc.getAddress(); save();
  }
  log(`${c.name}: factory = ${st.factory}`);

  // 2) ISM mutável (proxy; owner = signer)
  if (!st.ism) {
    const f = new ethers.Contract(st.factory, ["function deploy(address[],uint8) returns (address)"], w);
    const predicted = await f.getFunction("deploy(address[],uint8)").staticCall(VALIDATORS, THRESHOLD);
    const tx = await f.getFunction("deploy(address[],uint8)")(VALIDATORS, THRESHOLD, opts);
    log(`${c.name}: ism deploy tx ${tx.hash} …`); await tx.wait();
    st.ism = predicted; save();
  }
  const ism = ethers.getAddress(st.ism);
  const [vals, t] = await new ethers.Contract(ism, ISM_ABI, provider).validatorsAndThreshold("0x");
  const iOwner = await new ethers.Contract(ism, ISM_ABI, provider).owner();
  log(`${c.name}: ISM = ${ism} · ${vals.length} validadores · threshold ${t} · owner ${iOwner}`);
  if (Number(t) !== THRESHOLD || vals.length !== VALIDATORS.length) { log(`${c.name}: ❌ ISM não confere — abortando`); return; }

  // 3) warp
  const wc = new ethers.Contract(warp, WARP_ABI, w);
  if ((await wc.interchainSecurityModule()).toLowerCase() !== ism.toLowerCase()) {
    const tx = await wc.setInterchainSecurityModule(ism, opts);
    log(`${c.name}: warp.setInterchainSecurityModule tx ${tx.hash} …`); await tx.wait();
  }
  log(`${c.name}: ✓ warp usa ${await wc.interchainSecurityModule()}`);

  // 4) vault de recibo (se houver)
  if (c.vault) {
    const vc = new ethers.Contract(ethers.getAddress(c.vault), VAULT_ABI, w);
    if ((await vc.interchainSecurityModule()).toLowerCase() !== ism.toLowerCase()) {
      const tx = await vc.setIsm(ism, opts);
      log(`${c.name}: vault.setIsm tx ${tx.hash} …`); await tx.wait();
    }
    log(`${c.name}: ✓ vault usa ${await vc.interchainSecurityModule()}`);
  }
}

async function set(c) {
  const st = state[c.name];
  if (!st?.ism) { log(`${c.name}: ⚠ sem ISM em storage-ism.state — rode --deploy antes`); return; }
  const provider = new ethers.JsonRpcProvider(c.rpc);
  const ism = ethers.getAddress(st.ism);
  const ro = new ethers.Contract(ism, ISM_ABI, provider);
  const [vals, t] = await ro.validatorsAndThreshold("0x");
  log(`${c.name}: ISM ${ism} hoje: ${vals.length} validadores / t=${t} → novo: ${VALIDATORS.length} / t=${THRESHOLD}`);
  if (DRY) return;
  const w = signer(c, provider); if (!w) return;
  const tx = await new ethers.Contract(ism, ISM_ABI, w).setValidatorsAndThreshold(VALIDATORS, THRESHOLD, await optsFor(c, provider));
  log(`${c.name}: tx ${tx.hash} …`); await tx.wait();
  log(`${c.name}: ✓ atualizado — MESMO endereço ${ism}`);
}

async function show(c) {
  const provider = new ethers.JsonRpcProvider(c.rpc);
  const cur = await new ethers.Contract(ethers.getAddress(c.warp), WARP_ABI, provider).interchainSecurityModule();
  log(`\n${c.name}: warp → ISM ${cur}`);
  try {
    const [vals, t] = await new ethers.Contract(cur, ISM_ABI, provider).validatorsAndThreshold("0x");
    log(`  validadores (${vals.length}): ${vals.join(", ")}\n  threshold: ${t}`);
    try { log(`  owner: ${await new ethers.Contract(cur, ISM_ABI, provider).owner()} (MUTÁVEL)`); }
    catch { log("  owner: — (ESTÁTICO/imutável)"); }
  } catch { log("  (não expõe validatorsAndThreshold)"); }
  if (c.vault) log(`  vault → ISM ${await new ethers.Contract(ethers.getAddress(c.vault), VAULT_ABI, provider).interchainSecurityModule()}`);
}

const targets = ["eth", "bsc"].filter((k) => want("--" + k));
if (!targets.length) { log("indique as chains: --eth --bsc"); process.exit(1); }
for (const k of targets) {
  const c = CHAINS[k];
  if (want("--deploy")) await deploy(c);
  else if (want("--set")) await set(c);
  else await show(c);
}
log("\nfim.");
