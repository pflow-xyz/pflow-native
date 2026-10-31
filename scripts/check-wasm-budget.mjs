#!/usr/bin/env node
// check-wasm-budget.mjs: hold the browser build (`make wasm-shim`) against the
// budget pflow.xyz holds its vendored copy to, so a regression fails here
// before pflow.xyz vendors it.
//
//   node scripts/check-wasm-budget.mjs [path/to/pflow_native_wasm_bg.wasm]
//
// The limits copy WASM_BUDGET in pflow-xyz parity/composition-views/load_budget_test.ts
// (the reasons are in its docs/compose-load-budget.md); when pflow.xyz changes
// them, change them here too. The compression settings copy pflow-xyz
// scripts/sync-native-wasm.mjs, so the numbers are the ones its lock records.
// Exits 1 when any limit is exceeded.
import { appendFileSync, readFileSync } from 'node:fs';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const BUDGET = { brotli: 1500 * 1024, gzip: 2300 * 1024, raw: 10_500_000 };

const path = process.argv[2] ?? 'target/wasm-shim/pflow_native_wasm_bg.wasm';
const wasm = readFileSync(path);
const sizes = {
  brotli: brotliCompressSync(wasm, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: 11,
      [constants.BROTLI_PARAM_LGWIN]: 24,
      [constants.BROTLI_PARAM_SIZE_HINT]: wasm.length,
    },
  }).length,
  gzip: gzipSync(wasm, { level: 9 }).length,
  raw: wasm.length,
};

const rows = Object.keys(BUDGET).map((form) => {
  const over = sizes[form] > BUDGET[form];
  const pct = ((100 * sizes[form]) / BUDGET[form]).toFixed(1);
  return { form, size: sizes[form], budget: BUDGET[form], pct, over };
});

for (const r of rows) {
  console.log(`${r.form.padEnd(6)} ${String(r.size).padStart(9)} B of ${String(r.budget).padStart(9)} B (${r.pct}%)${r.over ? '  OVER BUDGET' : ''}`);
}

if (process.env.GITHUB_STEP_SUMMARY) {
  const table = rows.map((r) => `| ${r.form} | ${r.size.toLocaleString('en-US')} | ${r.budget.toLocaleString('en-US')} | ${r.pct}% | ${r.over ? 'over' : 'ok'} |`);
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    ['### wasm size against the pflow.xyz budget', '', '| form | bytes | budget | used | |', '|---|--:|--:|--:|---|', ...table, ''].join('\n') + '\n',
  );
}

if (rows.some((r) => r.over)) {
  console.error('wasm over budget: pflow.xyz would refuse this build (pflow-xyz docs/compose-load-budget.md)');
  process.exit(1);
}
