// Submissão no GasOracleGovernor.sol (BSC/Ethereum):
//   submitPrice(uint32 domain, uint128 tokenExchangeRate, uint128 gasPrice)
import { Contract, JsonRpcProvider, Wallet } from "ethers";

const ABI = [
  "function submitPrice(uint32 domain, uint128 tokenExchangeRate, uint128 gasPrice)",
  "function currentEpoch() view returns (uint256)",
];

export async function makeEvmSubmitter(chain) {
  const pk = process.env[chain.privateKeyEnv];
  if (!pk) throw new Error(`env ${chain.privateKeyEnv} ausente`);
  const provider = new JsonRpcProvider(chain.rpc);
  const wallet = new Wallet(pk, provider);
  const governor = new Contract(chain.governor, ABI, wallet);
  return {
    sender: wallet.address,
    async submit(domain, rate, gasPrice) {
      const tx = await governor.submitPrice(Number(domain), rate, gasPrice);
      const receipt = await tx.wait();
      return receipt.hash;
    },
  };
}
