// Submissão no oracle-governor CosmWasm (Terra Classic):
//   ExecuteMsg::SubmitPrice { domain, token_exchange_rate, gas_price }
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { GasPrice } from "@cosmjs/stargate";

export async function makeCosmwasmSubmitter(chain) {
  const mnemonic = process.env[chain.mnemonicEnv];
  if (!mnemonic) throw new Error(`env ${chain.mnemonicEnv} ausente`);
  const wallet = await DirectSecp256k1HdWallet.fromMnemonic(mnemonic, { prefix: chain.prefix });
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
