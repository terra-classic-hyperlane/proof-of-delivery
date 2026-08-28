// claim-agent — CLAIM phase of each round (runs alongside the oracle-agent):
//
//   TC/EVM : scans OUR relayer's deliveries in the Mailbox (events/logs) and
//            calls `Claim` on the vault with the message_ids not yet claimed.
//   Solana : counts the process() paid by the relayer in the Mailbox per EPOCH and, for
//            CLOSED epochs, submits `SubmitEpochReport` (on-chain quorum) and
//            then `Withdraw` of the available credit.
//
// State (cursors, pending ids, counted epochs) lives in state.json together with the
// price anchors. First round only writes the cursor (only NEW deliveries are
// claimed automatically; old ones: manual via docs/OPERACAO-CONTRATOS.md).
import { Connection, Keypair, PublicKey, SystemProgram, Transaction, TransactionInstruction } from "@solana/web3.js";

const log = (chain, msg) => console.log(`[${new Date().toISOString()}] [${chain}] [claims] ${msg}`);

/** queues ids for remote attestation in the TC vault (ClaimRemote v2) */
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
  // tx_search by the process event in the Mailbox; filters the sender = relayer
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
      const routerSender = evs.find((e) => e.type === "wasm-mailbox_process")
        ?.attributes.find((a) => a.key === "sender")?.value ?? "";
      const origin = Number((chain.claims.originSenders ?? {})[routerSender.toLowerCase()] ?? 0);
      for (const e of evs) {
        if (e.type !== "wasm-mailbox_process_id") continue;
        const id = e.attributes.find((a) => a.key === "message_id")?.value;
        if (id) ids.push({ id: id.replace(/^0x/, ""), origin: Number(origin ?? 0) });
      }
    }
    if (txs.length < 50) break;
    page += 1;
  }
  return ids;
}

/** Automatic sweep: when the IGP accumulates >= sweepMinUluna (default 100 LUNC),
 *  pulls the collected fees to the vault pool (permissionless). */
async function tcAutoSweep(chain, DRY) {
  if (!chain.claims.igp || !chain.claims.lcd) return;
  const r = await fetch(`${chain.claims.lcd}/cosmos/bank/v1beta1/balances/${chain.claims.igp}`).then((x) => x.json());
  const bal = BigInt(r.balances?.find((b) => b.denom === "uluna")?.amount ?? "0");
  const min = BigInt(chain.claims.sweepMinUluna ?? 100_000_000);
  if (bal < min) return;
  log("terraclassic", `IGP with ${Number(bal) / 1e6} LUNC accumulated` + (DRY ? " [dry-run: sweep not executed]" : " — running Sweep"));
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

/** Attests in the TC vault (v2 ClaimRemote) the confirmed remote deliveries from
 *  our relayers — the queue is fed by the BSC/ETH/Solana scanners. */
async function tcAttestRemote(chain, state, DRY) {
  const q = state.remoteAttest ?? {};
  const domains = Object.keys(q).filter((d) => (q[d] ?? []).length);
  if (!domains.length) return;
  const { CosmWasmClient, SigningCosmWasmClient } = await import("@cosmjs/cosmwasm-stargate");
  const ro = await CosmWasmClient.connect(chain.rpc);
  for (const d of domains) {
    // remove the ones already paid (idempotency across rounds)
    const ids = [];
    for (const id of q[d]) {
      const r = await ro.queryContractSmart(chain.claims.vault, { remote_claimed: { message_id: id } })
        .catch(() => null); // v1 still live → query does not exist
      if (r === null) { log("terraclassic", "vault still without v2 (ClaimRemote) — queue kept"); return; }
      if (!r.claimed) ids.push(id);
    }
    if (!ids.length) { q[d] = []; continue; }
    if (DRY) { log("terraclassic", `[dry-run] would attest ${ids.length} delivery(ies) from domain ${d}`); continue; }
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
      log("terraclassic", `✓ attested ${ids.length} remote delivery(ies) from domain ${d} → ${res.transactionHash}`);
    } catch (e) {
      log("terraclassic", `attestation dom ${d} ERROR — ${String(e.message).slice(0, 90)} (queue kept)`);
    }
  }
}

async function runTcClaims(chain, st, DRY, state, epochSecs) {
  await tcAutoSweep(chain, DRY).catch((e) => log("terraclassic", `sweep ERROR — ${e.message}`));
  await tcAttestRemote(chain, state, DRY).catch((e) => log("terraclassic", `attest ERROR — ${e.message}`));
  const height = await tcCurrentHeight(chain.rpc);
  if (st.cursor == null) {
    st.cursor = height;
    log("terraclassic", `initial cursor written at block ${height} — only new deliveries will be claimed`);
    return;
  }
  const novos = await tcScan(chain, st.cursor, height);
  st.cursor = height;
  // attestation queue in the ORIGIN EVM vaults (v2): deliveries ON TC of msgs
  // coming from BSC (56) / ETH (1) earn the origin fee there
  state.remoteAttestEvm = state.remoteAttestEvm ?? {};
  state.remoteAttestSol = state.remoteAttestSol ?? {};
  for (const n of novos) {
    if (n.origin === 56 || n.origin === 1) {
      const q = new Set(state.remoteAttestEvm[n.origin] ?? []);
      q.add(n.id);
      state.remoteAttestEvm[n.origin] = [...q];
    }
    if (n.origin === 1399811149) {
      // remote credit in the SOLANA vault, aggregated by its epoch
      const ep = Math.floor(Date.now() / 1000 / epochSecs);
      state.remoteAttestSol[ep] = (state.remoteAttestSol[ep] ?? 0) + 1;
    }
  }
  st.pending = [...new Set([...(st.pending ?? []), ...novos.map((n) => n.id)])];
  if (novos.length) log("terraclassic", `${novos.length} new delivery(ies) from the relayer detected`);
  if (!st.pending.length) return;

  const { CosmWasmClient, SigningCosmWasmClient } = await import("@cosmjs/cosmwasm-stargate");
  const ro = await CosmWasmClient.connect(chain.rpc);
  const claimables = [];
  for (const id of st.pending) {
    const c = await ro.queryContractSmart(chain.claims.vault, { claimed: { message_id: id } });
    if (c.claimed) log("terraclassic", `id ${id.slice(0, 12)}… already claimed by ${c.claimant} — removing`);
    else claimables.push(id);
  }
  st.pending = claimables;
  if (!claimables.length) return;

  if (chain.claims.localClaim === false) { return; }
  const sol = await ro.queryContractSmart(chain.claims.vault, { solvency: {} });
  const payable = Number(sol.claims_payable ?? sol.claimsPayable ?? 0);
  const lote = claimables.slice(0, Math.min(MAX_BATCH, payable));
  if (!lote.length) { log("terraclassic", `pool insufficient for ${claimables.length} claim(s) — pending kept`); return; }
  if (DRY) { log("terraclassic", `[dry-run] would claim ${lote.length} id(s)`); return; }

  const { DirectSecp256k1Wallet } = await import("@cosmjs/proto-signing");
  const { GasPrice } = await import("@cosmjs/stargate");
  const key = process.env[chain.privateKeyEnv];
  const wallet = await DirectSecp256k1Wallet.fromKey(Uint8Array.from(Buffer.from(key.replace(/^0x/, ""), "hex")), chain.prefix);
  const [acc] = await wallet.getAccounts();
  const client = await SigningCosmWasmClient.connectWithSigner(chain.rpc, wallet, { gasPrice: GasPrice.fromString(chain.gasPrice) });
  const res = await client.execute(acc.address, chain.claims.vault, { claim: { message_ids: lote } }, "auto");
  st.pending = claimables.filter((i) => !lote.includes(i));
  log("terraclassic", `✓ claim of ${lote.length} delivery(ies) → ${res.transactionHash}`);
}

// ---------------------------------------------------------------------------
// EVM (BSC / Ethereum)
// ---------------------------------------------------------------------------
async function runEvmClaims(name, chain, st, DRY, state) {
  const { Contract, JsonRpcProvider, Wallet, id } = await import("ethers");
  // TWO providers: claims.rpc ONLY for getLogs (RPCs with log support have
  // tight quotas); chain.rpc for calls and TRANSACTIONS (claim/attestation/igp).
  const providerLogs = new JsonRpcProvider(chain.claims.rpc ?? chain.rpc, undefined, { batchMaxCount: 1 });
  const provider = new JsonRpcProvider(chain.rpc, undefined, { batchMaxCount: 1 });
  const current = await provider.getBlockNumber();
  if (st.cursor == null) {
    st.cursor = current;
    log(name, `initial cursor written at block ${current} — only new deliveries will be claimed`);
    return;
  }
  const MAILBOX_ABI = [
    "event ProcessId(bytes32 indexed messageId)",
    "function processor(bytes32) view returns (address)",
    "function processedAt(bytes32) view returns (uint48)",
  ];
  const IGP_ABI = ["function claim()"];
  const VAULT_ABI = [
    "function claim(bytes32[] ids)",
    "function claimedBy(bytes32) view returns (address)",
    "function claimsPayable() view returns (uint256)",
    "function attestRemoteDelivery(uint32 domain, bytes32[] ids, address executor)",
    "function remoteClaimed(bytes32) view returns (address executor, uint32 domain, uint256 amount, uint256 blockNumber)",
    "function remoteReward(uint32) view returns (uint256)",
  ];
  const mailbox = new Contract(chain.claims.mailbox, MAILBOX_ABI, provider);
  const vault = new Contract(chain.claims.vault, VAULT_ABI, provider);

  // ---- auto-sweep: igp.claim() is permissionless and pushes the collected fees to the
  //      vault (beneficiary). Called when there is a relevant balance in the IGP.
  if (chain.claims.igp) {
    try {
      const igpBal = await provider.getBalance(chain.claims.igp);
      if (igpBal > 0n && !DRY) {
        const wallet = new Wallet(process.env[chain.privateKeyEnv], provider);
        const igp = new Contract(chain.claims.igp, IGP_ABI, wallet);
        const tx = await igp.claim();
        const rc = await tx.wait();
        log(name, `✓ igp.claim(): ${igpBal} wei → vault pool (${rc.hash})`);
      }
    } catch (e) {
      log(name, `igp.claim() ERROR — ${String(e.message).slice(0, 70)}`);
    }
  }

  // ---- v2: attest HERE the deliveries (made on TC) of msgs ORIGINATED here ----
  const attQ = (state.remoteAttestEvm ?? {})[chain.claims.domain] ?? [];
  if (attQ.length) {
    try {
      const reward = await vault.remoteReward(132556);
      if (reward === 0n) {
        log(name, `vault still without v2/reward — ${attQ.length} attestation(s) in the queue`);
      } else {
        const ids = [];
        for (const id of attQ) {
          const rc = await vault.remoteClaimed("0x" + id);
          if (rc[0] === "0x0000000000000000000000000000000000000000") ids.push("0x" + id);
        }
        if (ids.length && !DRY) {
          const wallet = new Wallet(process.env[chain.privateKeyEnv], provider);
          const tx = await vault.connect(wallet).attestRemoteDelivery(132556, ids, "0x0000000000000000000000000000000000000000");
          const rc = await tx.wait();
          log(name, `✓ attested ${ids.length} delivery(ies) on TC (origin ${chain.claims.domain}) → ${rc.hash}`);
        }
        state.remoteAttestEvm[chain.claims.domain] = [];
      }
    } catch (e) {
      log(name, `v2 attestation ERROR — ${String(e.message).slice(0, 80)} (queue kept)`);
    }
  }

  // scan in configurable windows (public RPCs vary A LOT in the eth_getLogs
  // limit: 1rpc/BSC = 50 blocks; mevblocker/ETH accepts hundreds)
  const chunk = Number(chain.claims.chunkBlocks ?? 2000);
  const maxWindows = Number(chain.claims.maxWindows ?? 30);
  const topic = id("ProcessId(bytes32)");
  let from = st.cursor + 1;
  const novos = [];
  for (let n = 0; n < maxWindows && from <= current; n++) {
    const to = Math.min(from + chunk - 1, current);
    try {
      const logs = await providerLogs.getLogs({ address: chain.claims.mailbox, topics: [topic], fromBlock: from, toBlock: to });
      for (const l of logs) novos.push(l.topics[1]);
      st.cursor = to;
      from = to + 1;
    } catch (e) {
      // intermittent public RPC: keep the cursor and continue on the NEXT round
      log(name, `scan interrupted at block ${from} (${String(e.message).slice(0, 60)}…) — resumes on the next round`);
      break;
    }
    await new Promise((r) => setTimeout(r, 150)); // pacing for rate limit
  }
  if (from <= current) log(name, `partial scan (up to block ${st.cursor}; continues on the next round)`);

  const pend = new Set(st.pending ?? []);
  const nossos = [];
  for (const mid of novos) {
    if ((await mailbox.processor(mid)).toLowerCase() === chain.claims.relayer.toLowerCase()) {
      pend.add(mid);
      nossos.push(mid);
    }
  }
  if (chain.claims.domain) queueRemoteAttest(state, chain.claims.domain, nossos);
  if (novos.length) log(name, `${novos.length} process() in the Mailbox · ${pend.size} from our relayer pending in total`);

  if (chain.claims.localClaim === false) { st.pending = []; return; }
  const claimables = [];
  for (const mid of pend) {
    if ((await vault.claimedBy(mid)) === "0x0000000000000000000000000000000000000000") claimables.push(mid);
  }
  st.pending = claimables;
  if (!claimables.length) return;

  const payable = Number(await vault.claimsPayable());
  const lote = claimables.slice(0, Math.min(MAX_BATCH, payable));
  if (!lote.length) { log(name, `pool insufficient for ${claimables.length} claim(s) — seed the vault; pending kept`); return; }
  if (DRY) { log(name, `[dry-run] would claim ${lote.length} id(s)`); return; }

  const wallet = new Wallet(process.env[chain.privateKeyEnv], provider);
  const tx = await vault.connect(wallet).claim(lote);
  const rc = await tx.wait();
  st.pending = claimables.filter((i) => !lote.includes(i));
  log(name, `✓ claim of ${lote.length} delivery(ies) → ${rc.hash}`);
}

// ---------------------------------------------------------------------------
// Solana — epoch report + withdraw (rrv module of the pod)
// ---------------------------------------------------------------------------
const sep = Buffer.from("-");
const u32le = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(Number(n)); return b; };
const u64le = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const rrvPda = (pod, seeds) => PublicKey.findProgramAddressSync([Buffer.from("rrv"), sep, ...seeds], pod)[0];

async function runSolanaClaims(chain, st, DRY, epochSecs, state) {
  const conn = new Connection(chain.rpc, "confirmed");
  const pod = new PublicKey(chain.governorProgram); // same program (pod)
  const mailbox = new PublicKey(chain.claims.mailboxProgram);
  const relayer = new PublicKey(chain.claims.relayer);

  // 1. new Mailbox signatures since the last cursor
  const opts = { limit: 100 };
  if (st.lastSig) opts.until = st.lastSig;
  const sigs = await conn.getSignaturesForAddress(mailbox, opts);
  if (!st.lastSig && sigs.length) {
    st.lastSig = sigs[0].signature;
    log("solana", `initial cursor written (${st.lastSig.slice(0, 12)}…) — only new deliveries will be counted`);
    return;
  }
  st.epochs = st.epochs ?? {};
  for (const s of sigs.reverse()) { // oldest first
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

  // 2. reports CLOSED epochs with deliveries
  const now = Math.floor(Date.now() / 1000);
  const currentEpoch = Math.floor(now / epochSecs);
  const kp = Keypair.fromSeed(Uint8Array.from(Buffer.from(process.env[chain.privateKeyEnv].replace(/^0x/, ""), "hex")));
  const remoteQ = state.remoteAttestSol ?? {};
  const DOM_TC = 132556;
  const epochsToReport = new Set([
    ...Object.keys(st.epochs),
    ...Object.keys(remoteQ),
  ].map(Number).filter((ep) => ep < currentEpoch));
  for (const epoch of epochsToReport) {
    const e = (st.epochs[epoch] = st.epochs[epoch] ?? { count: 0, minSlot: 0, maxSlot: 0, reported: false });
    const remoteCount = remoteQ[epoch] ?? 0;
    if (e.reported || (e.count === 0 && remoteCount === 0)) continue;
    log("solana", `epoch ${epoch} closed: ${e.count} local delivery(ies) + ${remoteCount} remote` + (DRY ? " [dry-run: not reported]" : ""));
    if (DRY) continue;
    // EpochReport { epoch, start, end, credits: [(op,count)], remote: [(dom,op,count)] }
    const credits = e.count > 0
      ? Buffer.concat([u32le(1), Buffer.from(relayer.toBytes()), u64le(e.count)])
      : u32le(0);
    const remote = remoteCount > 0
      ? Buffer.concat([u32le(1), u32le(DOM_TC), Buffer.from(relayer.toBytes()), u64le(remoteCount)])
      : u32le(0);
    const report = Buffer.concat([u64le(epoch), u64le(e.minSlot), u64le(e.maxSlot), credits, remote]);
    const creditPda = rrvPda(pod, [Buffer.from("credit"), sep, Buffer.from(relayer.toBytes())]);
    const keys = [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: rrvPda(pod, [Buffer.from("config")]), isSigner: false, isWritable: true },
      { pubkey: rrvPda(pod, [Buffer.from("epoch"), sep, u64le(epoch)]), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ];
    if (e.count > 0) keys.push({ pubkey: creditPda, isSigner: false, isWritable: true });
    if (remoteCount > 0) {
      keys.push({ pubkey: rrvPda(pod, [Buffer.from("rrew"), sep, u32le(DOM_TC)]), isSigner: false, isWritable: false });
      keys.push({ pubkey: rrvPda(pod, [Buffer.from("rbind"), sep, u32le(DOM_TC), sep, Buffer.from(relayer.toBytes())]), isSigner: false, isWritable: false });
      keys.push({ pubkey: creditPda, isSigner: false, isWritable: true });
    }
    const ix = new TransactionInstruction({
      programId: pod,
      keys,
      data: Buffer.concat([Buffer.from([0, 1]), report]), // [rrv module][SubmitEpochReport]
    });
    const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
    await conn.confirmTransaction(sig, "confirmed");
    e.reported = true;
    delete remoteQ[epoch];
    log("solana", `✓ report of epoch ${epoch} (${e.count} local + ${remoteCount} remote) → ${sig}`);
  }

  // 3. withdraw available credit (respecting the rent of the pool PDA)
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
  if (amount <= 0n) { log("solana", `credit of ${available} lamports but pool with no headroom — seed`); return; }
  if (DRY) { log("solana", `[dry-run] would withdraw ${amount} lamports of credit`); return; }
  const ix = new TransactionInstruction({
    programId: pod,
    keys: [
      { pubkey: kp.publicKey, isSigner: true, isWritable: true },
      { pubkey: configPda, isSigner: false, isWritable: true },
      { pubkey: creditPda, isSigner: false, isWritable: true },
    ],
    data: Buffer.concat([Buffer.from([0, 2]), u64le(amount)]), // [rrv module][Withdraw]
  });
  const sig = await conn.sendTransaction(new Transaction().add(ix), [kp]);
  await conn.confirmTransaction(sig, "confirmed");
  log("solana", `✓ withdraw of ${amount} lamports → ${sig}`);
}

// ---------------------------------------------------------------------------
export async function runClaims(name, chain, state, DRY, epochSecs) {
  if (!chain.claims?.enabled) return;
  const key = `claims:${name}`;
  const st = (state[key] = state[key] ?? {});
  try {
    if (chain.type === "cosmwasm") await runTcClaims(chain, st, DRY, state, epochSecs);
    else if (chain.type === "evm") await runEvmClaims(name, chain, st, DRY, state);
    else if (chain.type === "solana") await runSolanaClaims(chain, st, DRY, epochSecs, state);
  } catch (err) {
    log(name, `ERROR — ${err.message}`);
  }
}
