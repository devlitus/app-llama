import { test } from "node:test";
import assert from "node:assert/strict";
import { renderLong, selectLongEntries } from "./memory-long-format.mjs";

function closed(plan, { decisions = [], errors = [] } = {}) {
  const toEntry = ([slug, text = `${slug} ${plan}`]) => ({ slug, text, tasks: ["T1"] });
  return { plan, estado: "cerrado", patterns: { decisions: decisions.map(toEntry), errors: errors.map(toEntry) }, tasks: [] };
}

const open = (plan, decisions) => ({ ...closed(plan, { decisions }), estado: "abierto", tasks: [] });
const slugsOf = (entries) => entries.map(({ slug }) => slug);
const THREE = (decisions) => [closed("001-a", { decisions }), closed("002-b", { decisions }), closed("003-c", { decisions })];

test("long_umbral_tres_planes", () => {
  const docs = [
    closed("001-a", { decisions: [["a"], ["b"]] }),
    closed("002-b", { decisions: [["a"], ["b"]] }),
    closed("003-c", { decisions: [["a"]] }),
  ];
  assert.deepEqual(slugsOf(selectLongEntries(docs).decisions), ["a"]);
});

test("long_cuenta_planes_distintos_aunque_se_repita_el_slug", () => {
  const docs = [closed("001-a", { decisions: [["a"], ["a"]] }), closed("002-b", { decisions: [["a"], ["a"]] })];
  assert.deepEqual(selectLongEntries(docs).decisions, []);
  const three = [...docs, closed("003-c", { decisions: [["a"], ["a"]] })];
  assert.deepEqual(selectLongEntries(three).decisions[0].plans, ["001-a", "002-b", "003-c"]);
});

test("long_ignora_planes_abiertos", () => {
  const docs = [closed("001-a", { decisions: [["a"]] }), closed("002-b", { decisions: [["a"]] }), open("003-c", [["a"]])];
  assert.deepEqual(selectLongEntries(docs).decisions, []);
});

test("long_separa_tipos", () => {
  const docs = [
    closed("001-a", { decisions: [["x"]] }),
    closed("002-b", { decisions: [["x"]], errors: [["x"]] }),
    closed("003-c", { decisions: [["x"]], errors: [["x"]] }),
  ];
  const { decisions, errors } = selectLongEntries(docs);
  assert.deepEqual(slugsOf(decisions), ["x"]);
  assert.deepEqual(errors, []);
});

test("long_texto_del_plan_mas_reciente_y_planes_ascendentes", () => {
  const docs = [
    closed("001-a", { decisions: [["a", "v1"]] }),
    closed("004-d", { decisions: [["a", "v4"]] }),
    closed("002-b", { decisions: [["a", "v2"]] }),
  ];
  assert.deepEqual(selectLongEntries(docs).decisions, [{ slug: "a", text: "v4", plans: ["001-a", "002-b", "004-d"] }]);
});

test("long_orden_planes_desc_reciente_desc_slug_asc", () => {
  const docs = [
    closed("001-a", { decisions: [["m"], ["z"], ["a"], ["b"]] }),
    closed("002-b", { decisions: [["m"], ["z"], ["a"], ["b"]] }),
    closed("003-c", { decisions: [["m"], ["z"], ["a"], ["b"]] }),
    closed("004-d", { decisions: [["b"], ["a"]] }),
    closed("005-e", { decisions: [["z"]] }),
  ];
  const decisions = slugsOf(selectLongEntries(docs).decisions);
  assert.deepEqual(decisions, ["z", "a", "b", "m"]);
});

test("long_descarta_lo_que_no_cabe_en_40_entradas", () => {
  const slugs = Array.from({ length: 45 }, (_, i) => [`s${String(i).padStart(2, "0")}`]);
  const { decisions, dropped } = selectLongEntries(THREE(slugs));
  assert.equal(decisions.length, 40);
  assert.deepEqual(dropped, ["s40", "s41", "s42", "s43", "s44"].map((slug) => ({ kind: "decision", slug })));
});

test("long_descarta_errores_con_su_tipo", () => {
  const slugs = Array.from({ length: 41 }, (_, i) => [`e${String(i).padStart(2, "0")}`]);
  const docs = ["001-a", "002-b", "003-c"].map((plan) => closed(plan, { errors: slugs }));
  assert.deepEqual(selectLongEntries(docs).dropped, [{ kind: "error", slug: "e40" }]);
});

test("long_render_decisiones_exacto", () => {
  const entries = [{ slug: "cli-sin-dependencias", text: "Parseo a mano.", plans: ["001-memoria", "003-x", "005-y"] }];
  assert.equal(
    renderLong("decisiones", entries),
    `---
schema: memoria-larga/1
tipo: decisiones
---

# Decisiones recurrentes

<!-- Generado por \`node scripts/memory.mjs promote\`. No editar a mano. -->

- **cli-sin-dependencias** — Parseo a mano. (planes: 001-memoria, 003-x, 005-y)
`,
  );
});

test("long_render_errores_vacio", () => {
  const md = renderLong("errores", []);
  assert.match(md, /^---\nschema: memoria-larga\/1\ntipo: errores\n---\n\n# Errores recurrentes\n/);
  assert.match(md, /\n\n- \(ninguno\)\n$/);
  assert.match(renderLong("decisiones", []), /\n\n- \(ninguna\)\n$/);
  assert.equal(md.split("\n").length - 1, 10);
  assert.equal(renderLong("decisiones", []).split("\n").length - 1, 10);
});

test("long_render_con_40_entradas_cabe_en_49_lineas", () => {
  const entries = Array.from({ length: 40 }, (_, i) => ({ slug: `s${i}`, text: "t", plans: ["001-a", "002-b", "003-c"] }));
  assert.equal(renderLong("decisiones", entries).split("\n").length - 1, 49);
});
