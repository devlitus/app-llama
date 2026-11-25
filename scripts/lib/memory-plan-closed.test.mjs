import { test } from "node:test";
import assert from "node:assert/strict";
import { compactPlan, parsePlan, renderPlan } from "./memory-plan.mjs";

const CYCLES = [
  { cycle: 1, fecha: "2026-11-25", developer: 1, reviewer: "r1", gate: "verde", veredicto: "CHANGES_REQUESTED" },
  { cycle: 2, fecha: "2026-11-26", developer: 1, reviewer: "r2", gate: "verde", veredicto: "APPROVED" },
];

const OPEN_TASK = {
  task: "T3",
  cycles: CYCLES,
  summaries: [
    { cycle: 1, role: "developer", text: "Implementa la CLI de session." },
    { cycle: 2, role: "reviewer", text: "Revisa la corrección de --task." },
  ],
  decisions: [{ slug: "cli-sin-dependencias", text: "Parseo de argumentos a mano para no añadir dependencias.", cycles: [1] }],
  errors: [{ slug: "validacion-args", text: "No se valida el formato de --task. #12", cycles: [1], issues: [12] }],
};

const OPEN = { plan: "001-memoria", estado: "abierto", tasks: [OPEN_TASK] };

const CLOSED = {
  plan: "001-memoria",
  estado: "cerrado",
  patterns: {
    decisions: [
      { slug: "cli-sin-dependencias", text: "Parseo de argumentos a mano para no añadir dependencias.", tasks: ["T3"] },
    ],
    errors: [{ slug: "validacion-args", text: "No se valida el formato de --task. #12", tasks: ["T3"], issues: [12] }],
  },
  tasks: [{ task: "T3", cycles: CYCLES }],
};

const CLOSED_MD = `---
schema: memoria-plan/2
plan: 001-memoria
estado: cerrado
---

# Memoria del plan 001-memoria

## Patrones del plan

### Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (T3)

### Errores

- **validacion-args** — No se valida el formato de --task. #12 (T3 · #12)

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-25 | 1 | r1 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | r2 | verde | APPROVED |
`;

const EMPTY_CLOSED = { plan: "001-memoria", estado: "cerrado", patterns: { decisions: [], errors: [] }, tasks: [] };

const taskWith = (task, fields) => ({ ...OPEN_TASK, task, decisions: [], errors: [], ...fields });
const openWith = (...tasks) => ({ ...OPEN, tasks });

test("plan_render_cerrado_exacto", () => {
  assert.equal(renderPlan(CLOSED), CLOSED_MD);
});

test("plan_roundtrip_cerrado", () => {
  assert.deepEqual(parsePlan(renderPlan(CLOSED)), CLOSED);
  assert.deepEqual(parsePlan(CLOSED_MD.replace(/\n/g, "\r\n")), CLOSED);
});

test("plan_cerrado_vacio_roundtrip", () => {
  const md = renderPlan(EMPTY_CLOSED);
  assert.match(md, /### Decisiones\n\n- \(ninguna\)\n\n### Errores\n\n- \(ninguno\)\n$/);
  assert.deepEqual(parsePlan(md), EMPTY_CLOSED);
});

test("plan_cerrado_texto_con_parentesis", () => {
  const doc = structuredClone(CLOSED);
  doc.patterns.decisions[0].text = "usa (a) y (T1)";
  assert.deepEqual(parsePlan(renderPlan(doc)), doc);
});

test("plan_parse_cerrado_no_canonico", () => {
  assert.throws(() => parsePlan(CLOSED_MD.replace("### Ciclos\n", "### Ciclos\n\nnota suelta\n")), /canónico/);
});

test("plan_parse_cerrado_con_resumenes_no_es_canonico", () => {
  assert.throws(() => parsePlan(`${CLOSED_MD}\n### Resúmenes\n\n- C1 · developer · x\n`), /canónico/);
});

test("plan_parse_cerrado_sin_patrones", () => {
  const withoutPatterns = CLOSED_MD.replace(/## Patrones del plan[\s\S]*?(?=## T3)/, "");
  assert.throws(() => parsePlan(withoutPatterns), /Patrones del plan/);
});

test("plan_parse_cerrado_entrada_mal_formada", () => {
  assert.throws(() => parsePlan(CLOSED_MD.replace(" (T3)\n", "\n")), /entrada/);
});

test("plan_parse_abierto_rechaza_patrones", () => {
  const open = renderPlan(OPEN).replace("## T3", "## Patrones del plan\n\n### Decisiones\n\n- (ninguna)\n\n## T3");
  assert.throws(() => parsePlan(open), /tarea/);
});

test("compact_conserva_solo_ciclos_y_agrega_patrones", () => {
  assert.deepEqual(compactPlan(OPEN), CLOSED);
});

test("compact_texto_de_la_ultima_tarea", () => {
  const doc = openWith(
    taskWith("T1", { decisions: [{ slug: "a", text: "v1", cycles: [1] }] }),
    taskWith("T3", { decisions: [{ slug: "a", text: "v3", cycles: [1, 2] }] }),
  );
  assert.deepEqual(compactPlan(doc).patterns.decisions, [{ slug: "a", text: "v3", tasks: ["T1", "T3"] }]);
});

test("compact_une_issues_de_varias_tareas", () => {
  const doc = openWith(
    taskWith("T2", { errors: [{ slug: "x", text: "e #7", cycles: [1], issues: [7] }] }),
    taskWith("T4", { errors: [{ slug: "x", text: "e #12", cycles: [1], issues: [12] }] }),
  );
  const compact = compactPlan(doc);
  assert.deepEqual(compact.patterns.errors, [{ slug: "x", text: "e #12", tasks: ["T2", "T4"], issues: [7, 12] }]);
  assert.match(renderPlan(compact), /\(T2, T4 · #7, #12\)/);
});

test("compact_orden_numerico_de_tareas", () => {
  const entry = (text) => ({ decisions: [{ slug: "a", text, cycles: [1] }] });
  const compact = compactPlan(openWith(taskWith("T10", entry("v10")), taskWith("T2", entry("v2"))));
  assert.deepEqual(compact.patterns.decisions, [{ slug: "a", text: "v10", tasks: ["T2", "T10"] }]);
  assert.deepEqual(compact.tasks.map(({ task }) => task), ["T2", "T10"]);
});

test("compact_slugs_en_orden_alfabetico", () => {
  const doc = openWith(
    taskWith("T1", { decisions: [{ slug: "b", text: "x", cycles: [1] }, { slug: "a", text: "y", cycles: [1] }] }),
  );
  assert.deepEqual(compactPlan(doc).patterns.decisions.map(({ slug }) => slug), ["a", "b"]);
});

test("compact_no_muta_la_entrada", () => {
  const copy = structuredClone(OPEN);
  compactPlan(OPEN);
  assert.deepEqual(OPEN, copy);
});

const ORDER_ERROR = /repetido o desordenado/;

function closedWithSlugs(field, slugs) {
  const doc = structuredClone(CLOSED);
  doc.patterns[field] = slugs.map((slug) => ({ slug, text: "x", tasks: ["T3"], issues: [] }));
  return doc;
}

test("plan_parse_cerrado_rechaza_slugs_repetidos", () => {
  assert.throws(() => parsePlan(renderPlan(closedWithSlugs("decisions", ["a", "a", "a"]))), ORDER_ERROR);
});

test("plan_parse_cerrado_rechaza_slugs_desordenados", () => {
  assert.throws(() => parsePlan(renderPlan(closedWithSlugs("errors", ["b", "a"]))), ORDER_ERROR);
});

test("plan_parse_cerrado_rechaza_tareas_desordenadas", () => {
  const doc = { ...CLOSED, tasks: [{ task: "T10", cycles: CYCLES }, { task: "T2", cycles: CYCLES }] };
  assert.throws(() => parsePlan(renderPlan(doc)), ORDER_ERROR);
});
