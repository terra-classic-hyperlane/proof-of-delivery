import { test } from "node:test";
import assert from "node:assert/strict";
import { exchangeRate, SCALE_BY_VM } from "./prices.js";

test("EVM/CW scale is 1e10 and Solana is 1e19 (spec §08)", () => {
  assert.equal(SCALE_BY_VM.evm, 10n ** 10n);
  assert.equal(SCALE_BY_VM.cosmwasm, 10n ** 10n);
  assert.equal(SCALE_BY_VM.solana, 10n ** 19n);
});

test("BNB quoted in LUNC (cheap local coin → giant rate)", () => {
  // BNB $600 · LUNC $0.00006 → 10 million LUNC per BNB
  const rate = exchangeRate(600, 0.00006, "cosmwasm");
  assert.equal(rate, 10_000_000n * 10n ** 10n);
});

test("LUNC quoted in BNB (cheap remote → small fraction, without zeroing out)", () => {
  const rate = exchangeRate(0.00006, 600, "evm");
  // 1e-7 × 1e10 = 1000
  assert.equal(rate, 1000n);
});

test("same pair on Solana uses 1e19", () => {
  const rate = exchangeRate(0.00006, 150, "solana");
  // 4e-7 × 1e19 = 4e12
  assert.equal(rate, 4n * 10n ** 12n);
});

test("parity = exact scale", () => {
  assert.equal(exchangeRate(1, 1, "evm"), 10n ** 10n);
});
