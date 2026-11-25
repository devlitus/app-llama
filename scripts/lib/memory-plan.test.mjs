import { test } from "node:test";
import assert from "node:assert/strict";
import { mergeCycle, parsePlan, renderPlan } from "./memory-plan.mjs";

const EMPTY = { plan: "001-memoria", estado: "abierto", tasks: [] };

function session({ agente, agentId, n = 1, fecha = "2026-11-25", gate = "verde", veredicto, input = {} }) {
  const meta = { fecha, plan: "001-memoria", tarea: "T3", agente, agentId, n, rama: "chore/001-memoria", gate, files: [] };
  if (veredicto) meta.veredicto = veredicto;
  return { meta, input: { summary: `${agente} ${n}`, decisions: [], errors: [], ...input } };
}

const developer = (options = {}) => session({ agente: "developer", agentId: `d${options.n ?? 1}`, ...options });
const reviewer = (options = {}) =>
  session({ agente: "reviewer", agentId: "r1", veredicto: "APPROVED", ...options });

const SAMPLE = {
  plan: "001-memoria",
  estado: "abierto",
  tasks: [
    {
      task: "T3",
      cycles: [
        { cycle: 1, fecha: "2026-11-25", developer: 1, reviewer: "r1", gate: "verde", veredicto: "CHANGES_REQUESTED" },
        { cycle: 2, fecha: "2026-11-26", developer: 1, reviewer: "r2", gate: "verde", veredicto: "APPROVED" },
      ],
      summaries: [
        { cycle: 1, role: "developer", text: "Implementa la CLI de session." },
        { cycle: 1, role: "reviewer", text: "Falta validar el formato de --task." },
        { cycle: 2, role: "developer", text: "Valida --task con ^T\\d+$." },
        { cycle: 2, role: "reviewer", text: "Revisa la corrección de --task." },
      ],
      decisions: [
        { slug: "cli-sin-dependencias", text: "Parseo de argumentos a mano para no añadir dependencias.", cycles: [1] },
      ],
      errors: [
        { slug: "validacion-args", text: "No se valida el formato de --task. #12", cycles: [1], issues: [12] },
      ],
    },
  ],
};

const SAMPLE_MD = `---
schema: memoria-plan/2
plan: 001-memoria
estado: abierto
---

# Memoria del plan 001-memoria

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-25 | 1 | r1 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | r2 | verde | APPROVED |

### Resúmenes

- C1 · developer · Implementa la CLI de session.
- C1 · reviewer · Falta validar el formato de --task.
- C2 · developer · Valida --task con ^T\\d+$.
- C2 · reviewer · Revisa la corrección de --task.

### Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (C1)

### Errores

- **validacion-args** — No se valida el formato de --task. #12 (C1 · #12)
`;

test("plan_render_exacto", () => {
  assert.equal(renderPlan(SAMPLE), SAMPLE_MD);
});

test("plan_roundtrip_abierto", () => {
  assert.deepEqual(parsePlan(renderPlan(SAMPLE)), SAMPLE);
  assert.deepEqual(parsePlan(SAMPLE_MD.replace(/\n/g, "\r\n")), SAMPLE);
});

test("plan_texto_con_parentesis", () => {
  const text = "usa (a) y (b)";
  const doc = structuredClone(SAMPLE);
  doc.tasks[0].decisions[0].text = text;
  doc.tasks[0].errors[0] = { slug: "e", text: `${text} #3`, cycles: [1, 2], issues: [3] };
  const parsed = parsePlan(renderPlan(doc));
  assert.equal(parsed.tasks[0].decisions[0].text, text);
  assert.deepEqual(parsed.tasks[0].errors[0], doc.tasks[0].errors[0]);
});

test("plan_listas_vacias", () => {
  const doc = structuredClone(SAMPLE);
  doc.tasks[0].decisions = [];
  doc.tasks[0].errors = [];
  const md = renderPlan(doc);
  assert.match(md, /### Decisiones\n\n- \(ninguna\)\n/);
  assert.match(md, /### Errores\n\n- \(ninguno\)\n/);
  assert.deepEqual(parsePlan(md), doc);
});

test("plan_sin_tareas_roundtrip", () => {
  assert.deepEqual(parsePlan(renderPlan(EMPTY)), EMPTY);
});

test("plan_parse_sin_front_matter", () => {
  assert.throws(() => parsePlan("# hola"), /front matter/);
});

test("plan_parse_schema_desconocido", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("memoria-plan/2", "memoria-plan/1")), /schema/);
});

test("plan_parse_cerrado_solo_cabecera", () => {
  const closed = "---\nschema: memoria-plan/2\nplan: 001-memoria\nestado: cerrado\n---\n\n# Memoria del plan 001-memoria\n";
  assert.equal(parsePlan(closed).estado, "cerrado");
});

test("merge_primer_ciclo", () => {
  const doc = mergeCycle(EMPTY, "T3", [developer()], reviewer({ veredicto: "CHANGES_REQUESTED" }));
  assert.deepEqual(doc.tasks[0].cycles, [
    { cycle: 1, fecha: "2026-11-25", developer: 1, reviewer: "r1", gate: "verde", veredicto: "CHANGES_REQUESTED" },
  ]);
  assert.deepEqual(doc.tasks[0].summaries, [
    { cycle: 1, role: "developer", text: "developer 1" },
    { cycle: 1, role: "reviewer", text: "reviewer 1" },
  ]);
  assert.deepEqual(EMPTY.tasks, []);
});

test("merge_sin_developer", () => {
  const doc = mergeCycle(EMPTY, "T3", [], reviewer());
  assert.equal(doc.tasks[0].cycles[0].developer, 0);
});

test("merge_gate_rojo", () => {
  const doc = mergeCycle(EMPTY, "T3", [], reviewer({ gate: "rojo" }));
  assert.equal(doc.tasks[0].cycles[0].gate, "rojo");
});

test("merge_acumula_slug_y_issues", () => {
  const first = reviewer({
    veredicto: "CHANGES_REQUESTED",
    input: { errors: [{ slug: "validacion-args", text: "falla #12", issues: [12] }] },
  });
  const second = reviewer({
    agentId: "r2",
    fecha: "2026-11-26",
    input: { errors: [{ slug: "validacion-args", text: "texto nuevo #15", issues: [15] }] },
  });
  const doc = mergeCycle(mergeCycle(EMPTY, "T3", [], first), "T3", [], second);
  assert.deepEqual(doc.tasks[0].errors, [
    { slug: "validacion-args", text: "texto nuevo #15", cycles: [1, 2], issues: [12, 15] },
  ]);
  assert.match(renderPlan(doc), /- \*\*validacion-args\*\* — texto nuevo #15 \(C1, C2 · #12, #15\)/);
});

test("merge_slug_repetido_en_el_mismo_ciclo_no_duplica_ciclo", () => {
  const dev = developer({ input: { decisions: [{ slug: "a", text: "dev" }] } });
  const rev = reviewer({ input: { decisions: [{ slug: "a", text: "rev" }] } });
  const doc = mergeCycle(EMPTY, "T3", [dev], rev);
  assert.deepEqual(doc.tasks[0].decisions, [{ slug: "a", text: "rev", cycles: [1] }]);
});

test("merge_slugs_en_orden_alfabetico", () => {
  const rev = reviewer({ input: { decisions: [{ slug: "b", text: "x" }, { slug: "a", text: "y" }] } });
  const doc = mergeCycle(EMPTY, "T3", [], rev);
  assert.deepEqual(doc.tasks[0].decisions.map(({ slug }) => slug), ["a", "b"]);
});

test("merge_orden_tareas_numerico", () => {
  const afterT10 = mergeCycle(EMPTY, "T10", [], reviewer());
  const doc = mergeCycle(afterT10, "T2", [], reviewer({ agentId: "r2" }));
  assert.deepEqual(doc.tasks.map(({ task }) => task), ["T2", "T10"]);
});

test("merge_orden_developer_por_n_numerico", () => {
  const devs = [developer({ n: 10 }), developer({ n: 2 })];
  const doc = mergeCycle(EMPTY, "T3", devs, reviewer());
  assert.deepEqual(
    doc.tasks[0].summaries.map(({ text }) => text),
    ["developer 2", "developer 10", "reviewer 1"],
  );
});

test("merge_orden_developer_por_fecha_antes_que_n", () => {
  const devs = [developer({ n: 1, fecha: "2026-11-26" }), developer({ n: 2, fecha: "2026-11-25" })];
  const doc = mergeCycle(EMPTY, "T3", devs, reviewer());
  assert.deepEqual(
    doc.tasks[0].summaries.map(({ text }) => text),
    ["developer 2", "developer 1", "reviewer 1"],
  );
});

test("merge_el_texto_final_es_el_del_ultimo_developer_en_orden_numerico", () => {
  const devs = [
    developer({ n: 10, input: { decisions: [{ slug: "a", text: "n10" }] } }),
    developer({ n: 2, input: { decisions: [{ slug: "a", text: "n2" }] } }),
  ];
  const doc = mergeCycle(EMPTY, "T3", devs, reviewer());
  assert.equal(doc.tasks[0].decisions[0].text, "n10");
});

test("plan_parse_estado_desconocido", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("estado: abierto", "estado: Abierto")), /estado/);
});

test("plan_parse_resumen_mal_formado", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("- C1 · reviewer ·", "- C1 ·  reviewer ·")), /resumen/);
});

test("plan_parse_fila_de_ciclos_no_numerica", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("| 1 | 2026-11-25 | 1 |", "| 1 | 2026-11-25 | x |")), /ciclos/);
});

test("plan_parse_entrada_mal_formada", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace(" (C1)\n", "\n")), /entrada/);
});

test("plan_parse_tarea_con_nombre_invalido", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("## T3", "## Tres")), /tarea/);
});

test("plan_parse_no_canonico", () => {
  assert.throws(() => parsePlan(SAMPLE_MD.replace("### Ciclos\n", "### Ciclos\n\nnota suelta\n")), /canónico/);
});
