// Generates the *.layout.json fixtures: pflow.xyz's own diagram-viewer.js
// (translatePetriNet, unmodified) run under node on each input net.
//
//   node gen.mjs <path to pflow-xyz/public/diagram-viewer.js>
//
// Inputs, next to this file (or ../../../../../examples for Connect-4):
//   *.flat.json  what `bundle.flatten` returns; converted to diagram-viewer.js's
//                model exactly as pflow-views-prototype js/views/string.js `dvModel`
//                does (read arcs left out, weight [a.weight || 1], inhibitor flag).
//   *.net.json   a pflow net (places/transitions objects, arcs with source/target),
//                handed over as it is.
// Output *.layout.json: {input, translatePetriNet's result (boxes with all their
// fields, wires as rows of wireFields, isCrossing included, boundaries, backEdges,
// width, height)}. See README.md for provenance.
import { readFileSync, writeFileSync, createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dvPath = resolve(process.argv[2] || '');
const { translatePetriNet } = await import(pathToFileURL(dvPath).href);

const dvModel = (flat) => ({
  places: Object.fromEntries(flat.places.map((p) => [p.id, {}])),
  transitions: Object.fromEntries(flat.transitions.map((t) => [t.id, {}])),
  arcs: flat.arcs.filter((a) => a.type !== 'read').map((a) => ({ source: a.from, target: a.to, weight: [a.weight || 1], inhibitTransition: a.type === 'inhibitor' })),
});

const cases = [
  ['orders-default', 'flat', join(here, 'orders-default.flat.json')],
  ['orders-payment2-pushout', 'flat', join(here, 'orders-payment2-pushout.flat.json')],
  ['connect-4', 'net', join(here, '../../../../../examples/connect-4.json')],
  ['sir', 'net', join(here, 'sir.net.json')],
  ['coffee-shop', 'net', join(here, 'coffee-shop.net.json')],
  ['mixed', 'net', join(here, 'mixed.net.json')],
];
for (const [name, kind, file] of cases) {
  const raw = readFileSync(file);
  const j = JSON.parse(raw);
  const model = kind === 'flat' ? dvModel(j) : j;
  const d = translatePetriNet(model);
  // Wires are stored as rows (Connect-4 has 14,296 of them): see wireFields.
  const wireFields = ['wireType', 'fromX', 'fromY', 'toX', 'toY', 'isCrossing', 'isBack', 'inhibit', 'weight0'];
  const wires = d.wires.map((w) => [w.wireType, w.fromX, w.fromY, w.toX, w.toY, w.isCrossing, w.isBack, w.inhibit, w.weight[0]]);
  const out = {
    input: { file: file.slice(here.length + 1).replace(/^\.\.\/\.\.\/\.\.\/\.\.\/\.\.\//, ''), kind, bytes: raw.length, sha256: createHash('sha256').update(raw).digest('hex') },
    ...d,
    wireFields,
    wires,
  };
  writeFileSync(join(here, `${name}.layout.json`), JSON.stringify(out) + '\n');
  console.log(name, d.boxes.length, 'boxes', d.wires.length, 'wires', d.backEdges.length, 'back edges');
}
