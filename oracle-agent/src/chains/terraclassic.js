// Submission to the oracle-governor CosmWasm (Terra Classic):
//   ExecuteMsg::SubmitPrice { domain, token_exchange_rate, gas_price }
// Key: raw HEX (privateKeyEnv) — Hyperlane relayer format (cosmosKey).
// (mnemonicEnv is still accepted as an alternative.)
import { CosmWasmClient, SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet, DirectSecp256k1Wallet } from "@cosmjs/proto-signing";
import { GasPrice } from "@cosmjs/stargate";

/** CURRENT value of the production oracle (query without key). */
export async function readOracle(chain, domain) {
  const client = await CosmWasmClient.connect(chain.rpc);
  const res = await client.queryContractSmart(chain.oracle, {
    oracle: { get_exchange_rate_and_gas_price: { dest_domain: Number(domain) } },
  });
  return { rate: BigInt(res.exchange_rate), gas: BigInt(res.gas_price) };
}

export async function makeCosmwasmSubmitter(chain) {
  let wallet;
  const rawHex = chain.privateKeyEnv && process.env[chain.privateKeyEnv];
  if (rawHex) {
    const bytes = Uint8Array.from(Buffer.from(rawHex.replace(/^0x/, ""), "hex"));
    wallet = await DirectSecp256k1Wallet.fromKey(bytes, chain.prefix);
  } else {
    const mnemonic = process.env[chain.mnemonicEnv];
    if (!mnemonic) throw new Error(`env ${chain.privateKeyEnv ?? chain.mnemonicEnv} missing`);
    wallet = await DirectSecp256k1HdWallet.fromMnemonic(mnemonic, { prefix: chain.prefix });
  }
  const [account] = await wallet.getAccounts();
  const client = await SigningCosmWasmClient.connectWithSigner(chain.rpc, wallet, {
    gasPrice: GasPrice.fromString(chain.gasPrice),
  });
  return {
    sender: account.address,
    async submit(domain, rate, gasPrice) {
      const msg = {
        submit_price: {
          domain: Number(domain),
          token_exchange_rate: rate.toString(),
          gas_price: gasPrice.toString(),
        },
      };
      const res = await client.execute(account.address, chain.governor, msg, "auto");
      return res.transactionHash;
    },
  };
}
