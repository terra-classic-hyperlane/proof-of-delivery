import { test } from "node:test";
import assert from "node:assert/strict";
import { exchangeRate, SCALE_BY_VM } from "./prices.js";

test("escala EVM/CW é 1e10 e Solana é 1e19 (spec §08)", () => {
  assert.equal(SCALE_BY_VM.evm, 10n ** 10n);
  assert.equal(SCALE_BY_VM.cosmwasm, 10n ** 10n);
  assert.equal(SCALE_BY_VM.solana, 10n ** 19n);
});

test("BNB cotado em LUNC (moeda local barata → rate gigante)", () => {
  // BNB $600 · LUNC $0.00006 → 10 milhões de LUNC por BNB
  const rate = exchangeRate(600, 0.00006, "cosmwasm");
  assert.equal(rate, 10_000_000n * 10n ** 10n);
});

test("LUNC cotado em BNB (remoto barato → fração pequena, sem zerar)", () => {
  const rate = exchangeRate(0.00006, 600, "evm");
  // 1e-7 × 1e10 = 1000
  assert.equal(rate, 1000n);
});

test("mesmo par em Solana usa 1e19", () => {
  const rate = exchangeRate(0.00006, 150, "solana");
  // 4e-7 × 1e19 = 4e12
  assert.equal(rate, 4n * 10n ** 12n);
});

test("paridade = escala exata", () => {
  assert.equal(exchangeRate(1, 1, "evm"), 10n ** 10n);
});
