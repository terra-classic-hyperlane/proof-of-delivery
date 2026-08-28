// Submission to GasOracleGovernor.sol (BSC/Ethereum):
//   submitPrice(uint32 domain, uint128 tokenExchangeRate, uint128 gasPrice)
// Key: HEX (privateKeyEnv) — same as the Hyperlane relayer.
import { Contract, JsonRpcProvider, Wallet } from "ethers";

const GOV_ABI = [
  "function submitPrice(uint32 domain, uint128 tokenExchangeRate, uint128 gasPrice)",
  "function currentEpoch() view returns (uint256)",
];
const ORACLE_ABI = [
  "function getExchangeRateAndGasPrice(uint32) view returns (uint128 tokenExchangeRate, uint128 gasPrice)",
];

/** CURRENT value of the production oracle (call without key). */
export async function readOracle(chain, domain) {
  const provider = new JsonRpcProvider(chain.rpc);
  const oracle = new Contract(chain.oracle, ORACLE_ABI, provider);
  const [rate, gas] = await oracle.getExchangeRateAndGasPrice(Number(domain));
  return { rate: BigInt(rate), gas: BigInt(gas) };
}

export async function makeEvmSubmitter(chain) {
  const pk = process.env[chain.privateKeyEnv];
  if (!pk) throw new Error(`env ${chain.privateKeyEnv} missing`);
  const provider = new JsonRpcProvider(chain.rpc);
  const wallet = new Wallet(pk, provider);
  const governor = new Contract(chain.governor, GOV_ABI, wallet);
  return {
    sender: wallet.address,
    async submit(domain, rate, gasPrice) {
      const tx = await governor.submitPrice(Number(domain), rate, gasPrice);
      const receipt = await tx.wait();
      return receipt.hash;
    },
  };
}
