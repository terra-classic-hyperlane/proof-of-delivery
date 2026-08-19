// claim-agent — fase de RESGATE de cada rodada (roda junto do oracle-agent):
//
//   TC/EVM : varre as entregas do NOSSO relayer no Mailbox (eventos/logs) e
//            chama `Claim` no vault com os message_ids ainda não resgatados.
//   Solana : conta os process() pagos pelo relayer no Mailbox por ÉPOCA e, para
//            épocas FECHADAS, submete `SubmitEpochReport` (quórum on-chain) e
//            depois `Withdraw` do crédito disponível.
//
// Estado (cursors, ids pendentes, épocas contadas) vive no state.json junto das
// âncoras de preço. Primeira rodada só grava o cursor (só entregas NOVAS são
// resgatadas automaticamente; antigas: manual via docs/OPERACAO-CONTRATOS.md).
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const log = (chain, msg) => console.log(`[${new Date().toISOString()}] [${chain}] [claims] ${msg}`);

/** enfileira ids p/ atestação remota no vault do TC (ClaimRemote v2) */
function queueRemoteAttest(state, domain, ids) {
  if (!ids.length) return;
  state.remoteAttest = state.remoteAttest ?? {};
  const q = new Set(state.remoteAttest[domain] ?? []);
  for (const id of ids) q.add(id.replace(/^0x/, ""));
  state.remoteAttest[domain] = [...q];
}
const MAX_BATCH = 25;

// ---------------------------------------------------------------------------
// Terra Classic
// ---------------------------------------------------------------------------
async function tcCurrentHeight(rpc) {
  const r = await fetch(`${rpc}/status`).then((x) => x.json());
  return Number(r.result.sync_info.latest_block_height);
}

async function tcScan(chain, fromHeight, toHeight) {
  // tx_search pelo evento do process no Mailbox; filtra o sender = relayer
  const ids = [];
  let page = 1;
  for (;;) {
    const q = encodeURIComponent(
      `wasm-mailbox_process_id._contract_address='${chain.claims.mailbox}' AND tx.height>${fromHeight} AND tx.height<=${toHeight}`,
    );
    const r = await fetch(`${chain.rpc}/tx_search?query="${q}"&per_page=50&page=${page}`).then((x) => x.json());
    const txs = r.result?.txs ?? [];
    for (const t of txs) {
      if (t.tx_result.code !== 0 && t.tx_result.code !== undefined) continue;
      const evs = t.tx_result.events ?? [];
      const sender = evs.find((e) => e.type === "message" && e.attributes.some((a) => a.key === "action"))
        ?.attributes.find((a) => a.key === "sender")?.value;
      if (sender !== chain.claims.relayer) continue;
      for (const e of evs) {
        if (e.type !== "wasm-mailbox_process_id") continue;
        const id = e.attributes.find((a) => a.key === "message_id")?.value;
        if (id) ids.push(id.replace(/^0x/, ""));
      }
    }
    if (txs.length < 50) break;
    page += 1;
  }
  return ids;
}

/** Sweep automático: quando o IGP acumular >= sweepMinUluna (default 100 LUNC),
 *  puxa a arrecadação p/ o pool do vault (permissionless). */
async function tcAutoSweep(chain, DRY) {
  if (!chain.claims.igp || !chain.claims.lcd) return;
  const r = await fetch(`${chain.claims.lcd}/cosmos/bank/v1beta1/balances/${chain.claims.igp}`).then((x) => x.json());
  const bal = BigInt(r.balances?.find((b) => b.denom === "uluna")?.amount ?? "0");
  const min = BigInt(chain.claims.sweepMinUluna ?? 100_000_000);
  if (bal < min) return;
  log("terraclassic", `IGP com ${Number(bal) / 1e6} LUNC acumulados` + (DRY ? " [dry-run: sweep não executado]" : " — executando Sweep"));
  if (DRY) return;
  const { SigningCosmWasmClient } = await import("@cosmjs/cosmwasm-stargate");
  const { DirectSecp256k1Wallet } = await import("@cosmjs/proto-signing");
  const { GasPrice } = await import("@cosmjs/stargate");
  const key = process.env[chain.privateKeyEnv].replace(/^0x/, "");
  const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(key, "hex")), chain.prefix);
  const [acc] = await wallet.getAccounts();
  const client = await SigningCosmWasmClient.connectWithSigner(chain.rpc, wallet, { gasPrice: GasPrice.fromString(chain.gasPrice) });
  const res = await client.execute(acc.address, chain.claims.vault, { sweep: {} }, "auto");
  log("terraclassic", `✓ Sweep → pool: ${res.transactionHash}`);
}

/** Atesta no vault do TC (v2 ClaimRemote) as entregas remotas confirmadas dos
 *  nossos relayers — a fila é alimentada pelos scanners de BSC/ETH/Solana. */
async function tcAttestRemote(chain, state, DRY) {
  const q = state.remoteAttest ?? {};
  const domains = Object.keys(q).filter((d) => (q[d] ?? []).length);
  if (!domains.length) return;
  const { CosmWasmClient, SigningCosmWasmClient } = await import("@cosmjs/cosmwasm-stargate");
  const ro = await CosmWasmClient.connect(chain.rpc);
  for (const d of domains) {
    // remove os já pagos (idempotência entre rodadas)
    const ids = [];
    for (const id of q[d]) {
      const r = await ro.queryContractSmart(chain.claims.vault, { remote_claimed: { message_id: id } })
        .catch(() => null); // v1 ainda no ar → query inexistente
      if (r === null) { log("terraclassic", "vault ainda sem v2 (ClaimRemote) — fila mantida"); return; }
      if (!r.claimed) ids.push(id);
    }
    if (!ids.length) { q[d] = []; continue; }
    if (DRY) { log("terraclassic", `[dry-run] atestaria ${ids.length} entrega(s) do domínio ${d}`); continue; }
    const { DirectSecp256k1Wallet } = await import("@cosmjs/proto-signing");
    const { GasPrice } = await import("@cosmjs/stargate");
    const key = process.env[chain.privateKeyEnv].replace(/^0x/, "");
    const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(key, "hex")), chain.prefix);
    const [acc] = await wallet.getAccounts();
    const client = await SigningCosmWasmClient.connectWithSigner(chain.rpc, wallet, { gasPrice: GasPrice.fromString(chain.gasPrice) });
    try {
      const res = await client.execute(acc.address, chain.claims.vault,
        { attest_remote_delivery: { domain: Number(d), message_ids: ids.slice(0, MAX_BATCH), executor: null } }, "auto");
      q[d] = q[d].filter((i) => !ids.slice(0, MAX_BATCH).includes(i));
      log("terraclassic", `✓ atestado ${ids.length} entrega(s) remota(s) do domínio ${d} → ${res.transactionHash}`);
    } catch (e) {
      log("terraclassic", `atestação dom ${d} ERRO — ${String(e.message).slice(0, 90)} (fila mantida)`);
    }
  }
}

async function runTcClaims(chain, st, DRY, state) {
  await tcAutoSweep(chain, DRY).catch((e) => log("terraclassic", `sweep ERRO — ${e.message}`));
  await tcAttestRemote(chain, state, DRY).catch((e) => log("terraclassic", `attest ERRO — ${e.message}`));
  const height = await tcCurrentHeight(chain.rpc);
  if (st.cursor == null) {
    st.cursor = height;
    log("terraclassic", `cursor inicial gravado no bloco ${height} — só entregas novas serão resgatadas`);
    return;
  }
  const novos = await tcScan(chain, st.cursor, height);
  st.cursor = height;
  st.pending = [...new Set([...(st.pending ?? []), ...novos])];
  if (novos.length) log("terraclassic", `${novos.length} entrega(s) nova(s) do relayer detectada(s)`);
  if (!st.pending.length) return;

  const { CosmWasmClient, SigningCosmWasmClient } = await import("@cosmjs/cosmwasm-stargate");
  const ro = await CosmWasmClient.connect(chain.rpc);
  const claimables = [];
  for (const id of st.pending) {
    const c = await ro.queryContractSmart(chain.claims.vault, { claimed: { message_id: id } });
    if (c.claimed) log("terraclassic", `id ${id.slice(0, 12)}… já resgatado por ${c.claimant} — removendo`);
    else claimables.push(id);
  }
  st.pending = claimables;
  if (!claimables.length) return;

  const sol = await ro.queryContractSmart(chain.claims.vault, { solvency: {} });
  const payable = Number(sol.claims_payable ?? sol.claimsPayable ?? 0);
  const lote = claimables.slice(0, Math.min(MAX_BATCH, payable));
  if (!lote.length) { log("terraclassic", `pool insuficiente p/ ${claimables.length} claim(s) — pendentes mantidos`); return; }
  if (DRY) { log("terraclassic", `[dry-run] resgataria ${lote.length} id(s)`); return; }

  const { DirectSecp256k1Wallet } = await import("@cosmjs/proto-signing");
  const { GasPrice } = await import("@cosmjs/stargate");
  const key = process.env[chain.privateKeyEnv];
  const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(key.replace(/^0x/, ""), "hex")), chain.prefix);
  const [acc] = await wallet.getAccounts();
  const client = await SigningCosmWasmClient.connectWithSigner(chain.rpc, wallet, { gasPrice: GasPrice.fromString(chain.gasPrice) });
  const res = await client.execute(acc.address, chain.claims.vault, { claim: { message_ids: lote } }, "auto");
  st.pending = claimables.filter((i) => !lote.includes(i));
  log("terraclassic", `✓ claim de ${lote.length} entrega(s) → ${res.transactionHash}`);
}

// ---------------------------------------------------------------------------
// EVM (BSC / Ethereum)
// ---------------------------------------------------------------------------
async function runEvmClaims(name, chain, st, DRY, state) {
  const { Contract, JsonRpcProvider, Wallet, id } = await import("ethers");
  // RPC próprio p/ claims (getLogs): batch DESLIGADO (dataseed da BSC rejeita
  // eth_getLogs em batch) e endpoint alternativo onde o principal bloqueia logs.
  const provider = new JsonRpcProvider(chain.claims.rpc ?? chain.rpc, undefined, { batchMaxCount: 1 });
  const current = await provider.getBlockNumber();
  if (st.cursor == null) {
    st.cursor = current;
    log(name, `cursor inicial gravado no bloco ${current} — só entregas novas serão resgatadas`);
    return;
  }
  const MAILBOX_ABI = [
    "event ProcessId(bytes32 indexed messageId)",
    "function processor(bytes32) view returns (address)",
    "function processedAt(bytes32) view returns (uint48)",
  ];
  const VAULT_ABI = [
    "function claim(bytes32[] ids)",
    "function claimedBy(bytes32) view returns (address)",
    "function claimsPayable() view returns (uint256)",
  ];
  const mailbox = new Contract(chain.claims.mailbox, MAILBOX_ABI, provider);
  const vault = new Contract(chain.claims.vault, VAULT_ABI, provider);

  // varredura em janelas configuráveis (RPCs públicos variam MUITO no limite
  // de eth_getLogs: 1rpc/BSC = 50 blocos; mevblocker/ETH aceita centenas)
  const chunk = Number(chain.claims.chunkBlocks ?? 2000);
  const maxWindows = Number(chain.claims.maxWindows ?? 30);
  const topic = id("ProcessId(bytes32)");
  let from = st.cursor + 1;
  const novos = [];
  for (let n = 0; n < maxWindows && from <= current; n++) {
    const to = Math.min(from + chunk - 1, current);
    try {
      const logs = await provider.getLogs({ address: chain.claims.mailbox, topics: [topic], fromBlock: from, toBlock: to });
      for (const l of logs) novos.push(l.topics[1]);
      st.cursor = to;
      from = to + 1;
    } catch (e) {
      // RPC público intermitente: mantém o cursor e continua na PRÓXIMA rodada
      log(name, `varredura interrompida no bloco ${from} (${String(e.message).slice(0, 60)}…) — retoma na próxima rodada`);
      break;
    }
    await new Promise((r) => setTimeout(r, 150)); // pacing p/ rate limit
  }
  if (from <= current) log(name, `varredura parcial (até bloco ${st.cursor}; continua na próxima rodada)`);

  const pend = new Set(st.pending ?? []);
  const nossos = [];
  for (const mid of novos) {
    if ((await mailbox.processor(mid)).toLowerCase() === chain.claims.relayer.toLowerCase()) {
      pend.add(mid);
      nossos.push(mid);
    }
  }
  if (chain.claims.domain) queueRemoteAttest(state, chain.claims.domain, nossos);
  if (novos.length) log(name, `${novos.length} process() no Mailbox · ${pend.size} do nosso relayer no total pendente`);

  const claimables = [];
  for (const mid of pend) {
    if ((await vault.claimedBy(mid)) === "0x0000000000000000000000000000000000000000") claimables.push(mid);
  }
  st.pending = claimables;
  if (!claimables.length) return;

  const payable = Number(await vault.claimsPayable());
  const lote = claimables.slice(0, Math.min(MAX_BATCH, payable));
  if (!lote.length) { log(name, `pool insuficiente p/ ${claimables.length} claim(s) — semear o vault; pendentes mantidos`); return; }
  if (DRY) { log(name, `[dry-run] resgataria ${lote.length} id(s)`); return; }

  const wallet = new Wallet(process.env[chain.privateKeyEnv], provider);
  const tx = await vault.connect(wallet).claim(lote);
  const rc = await tx.wait();
  st.pending = claimables.filter((i) => !lote.includes(i));
  log(name, `✓ claim de ${lote.length} entrega(s) → ${rc.hash}`);
}

// ---------------------------------------------------------------------------
// Solana — relatório de época + withdraw (módulo rrv do pod)
// ---------------------------------------------------------------------------
const sep = Buffer.from("-");
const u32le = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
const u64le = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const rrvPda = (pod, seeds) => PublicKey.findProgramAddressSync([Buffer.from("rrv"), sep, ...seeds], pod)[0];

async function runSolanaClaims(chain, st, DRY, epochSecs, state) {
  const conn = new Connection(chain.rpc, "confirmed");
  const pod = new PublicKey(chain.governorProgram); // mesmo programa (pod)
  const mailbox = new PublicKey(chain.claims.mailboxProgram);
  const relayer = new PublicKey(chain.claims.relayer);

  // 1. novas assinaturas do Mailbox desde o último cursor
  const opts = { limit: 100 };
  if (st.lastSig) opts.until = st.lastSig;
  const sigs = await conn.getSignaturesForAddress(mailbox, opts);
  if (!st.lastSig && sigs.length) {
    st.lastSig = sigs[0].signature;
    log("solana", `cursor inicial gravado (${st.lastSig.slice(0, 12)}…) — só entregas novas serão contadas`);
    return;
  }
  st.epochs = st.epochs ?? {};
  for (const s of sigs.reverse()) { // mais antigas primeiro
    if (s.err) continue;
    const tx = await conn.getTransaction(s.signature, { maxSupportedTransactionVersion: 0 });
    if (!tx || tx.meta.err) continue;
    const keys = tx.transaction.message.staticAccountKeys ?? tx.transaction.message.accountKeys;
    if (!keys[0].equals(relayer)) continue; // fee payer = executor
    const mid = (tx.meta.logMessages || []).join(" ").match(/processed message (0x[0-9a-f]{64})/)?.[1];
    if (mid && chain.claims.domain) queueRemoteAttest(state, chain.claims.domain, [mid]);
    const epoch = Math.floor(tx.blockTime / epochSecs);
    const e = (st.epochs[epoch] = st.epochs[epoch] ?? { count: 0, minSlot: tx.slot, maxSlot: tx.slot, reported: false });
    e.count += 1;
    e.minSlot = Math.min(e.minSlot, tx.slot);
    e.maxSlot = Math.max(e.maxSlot, tx.slot);
    st.lastSig = s.signature;
  }

  // 2. reporta épocas FECHADAS com entregas
  const now = Math.floor(Date.now() / 1000);
  const currentEpoch = Math.floor(now / epochSecs);
  const kp = Keypair.fromSeed(Uint8Array.from(Buffer.from(process.env[chain.privateKeyEnv].replace(/^0x/, ""), "hex")));
  for (const [epochStr, e] of Object.entries(st.epochs)) {
    const epoch = Number(epochStr);
    if (e.reported || e.count === 0 || epoch >= currentEpoch) continue;
    log("solana", `época ${epoch} fechada: ${e.count} entrega(s) do relayer` + (DRY ? " [dry-run: não reportado]" : ""));
    if (DRY) continue;
    // EpochReport { epoch u64, start u64, end u64, credits: [(relayer, count)] }
    const report = Buffer.concat([
      u64le(epoch), u64le(e.minSlot), u64le(e.maxSlot),
      u32le(1), Buffer.from(relayer.toBytes()), u64le(e.count),
    ]);
    const ix = new TransactionInstruction({
      programId: pod,
      keys: [
        { pubkey: kp.publicKey, isSigner: true, isWritable: true },
        { pubkey: rrvPda(pod, [Buffer.from("config")]), isSigner: false, isWritable: true },
        { pubkey: rrvPda(pod, [Buffer.from("epoch"), sep, u64le(epoch)]), isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: rrvPda(pod, [Buffer.from("credit"), sep, Buffer.from(relayer.toBytes())]), isSigner: false, isWritable: true },
      ],
      data: Buffer.concat([Buffer.from([0, 1]), report]), // [módulo rrv][SubmitEpochReport]
    });
    const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    e.reported = true;
    log("solana", `✓ relatório da época ${epoch} → ${sig}`);
  }

  // 3. saca crédito disponível (respeitando o rent da PDA do pool)
  const creditPda = rrvPda(pod, [Buffer.from("credit"), sep, Buffer.from(relayer.toBytes())]);
  const cInfo = await conn.getAccountInfo(creditPda);
  if (!cInfo) return;
  const credited = cInfo.data.readBigUInt64LE(33);
  const withdrawn = cInfo.data.readBigUInt64LE(41);
  const available = credited - withdrawn;
  if (available <= 0n) return;
  const configPda = rrvPda(pod, [Buffer.from("config")]);
  const cfgInfo = await conn.getAccountInfo(configPda);
  const rentFloor = BigInt(await conn.getMinimumBalanceForRentExemption(cfgInfo.data.length));
  const pool = BigInt(cfgInfo.lamports) - rentFloor;
  const amount = available < pool ? available : pool;
  if (amount <= 0n) { log("solana", `crédito de ${available} lamports mas pool sem folga — semear`); return; }
  if (DRY) { log("solana", `[dry-run] sacaria ${amount} lamports de crédito`); return; }
  const ix = new TransactionInstruction({
    programId: pod,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: configPda, isSigner: false, isWritable: true },
      { pubkey: creditPda, isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.from([0, 2]), u64le(amount)]), // [módulo rrv][Withdraw]
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  log("solana", `✓ withdraw de ${amount} lamports → ${sig}`);
}

// ---------------------------------------------------------------------------
export async function runClaims(name, chain, state, DRY, epochSecs) {
  if (!chain.claims?.enabled) return;
  const key = `claims:${name}`;
  const st = (state[key] = state[key] ?? {});
  try {
    if (chain.type === "cosmwasm") await runTcClaims(chain, st, DRY, state);
    else if (chain.type === "evm") await runEvmClaims(name, chain, st, DRY, state);
    else if (chain.type === "solana") await runSolanaClaims(chain, st, DRY, epochSecs, state);
  } catch (err) {
    log(name, `ERRO — ${err.message}`);
  }
}
