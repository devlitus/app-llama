import { test } from "node:test";
import assert from "node:assert/strict";
import {
  MemoryInputError,
  formatDate,
  parseSession,
  parseSessionInput,
  renderSession,
  sessionFileName,
} from "./memory-entry.mjs";

const hfToken = "hf_" + "A".repeat(34);

function assertInputError(text, pattern) {
  assert.throws(
    () => parseSessionInput(text),
    (error) => error instanceof MemoryInputError && error.code === 2 && pattern.test(error.message),
  );
}

test("entrada_minima", () => {
  assert.deepEqual(parseSessionInput("resumen: Implementa X"), {
    summary: "Implementa X",
    decisions: [],
    errors: [],
  });
});

test("entrada_completa", () => {
  const text = [
    "resumen: Implementa X",
    "decision: [uno] Primera decisión",
    "decision: [dos] Segunda decisión",
    "error: [validacion-args] falla #12",
  ].join("\n");
  assert.deepEqual(parseSessionInput(text), {
    summary: "Implementa X",
    decisions: [
      { slug: "uno", text: "Primera decisión" },
      { slug: "dos", text: "Segunda decisión" },
    ],
    errors: [{ slug: "validacion-args", text: "falla #12", issues: [12] }],
  });
});

test("entrada_crlf_y_vacias", () => {
  const plain = parseSessionInput("resumen: a\ndecision: [x] y");
  assert.deepEqual(parseSessionInput("\r\nresumen: a\r\n\r\n\r\ndecision: [x] y\r\n\r\n"), plain);
});

test("entrada_issues_ordenados_unicos", () => {
  const { errors } = parseSessionInput("resumen: a\nerror: [x] ver #12, #7 y #12");
  assert.deepEqual(errors[0].issues, [7, 12]);
});

test("entrada_sin_resumen", () => {
  assertInputError("decision: [a] b", /falta 'resumen'/);
});

test("entrada_dos_resumenes", () => {
  assertInputError("resumen: a\nresumen: b", /solo se admite un 'resumen'/);
});

test("entrada_clave_desconocida", () => {
  assertInputError("resumen: a\nnota: b", /línea 2.*clave desconocida 'nota'/);
});

test("entrada_sin_slug", () => {
  assertInputError("resumen: a\ndecision: sin corchetes", /línea 2.*slug|slug.*línea 2/);
});

test("entrada_slug_invalido", () => {
  assertInputError("resumen: a\ndecision: [Rutas_Win] x", /slug inválido/);
});

test("entrada_slug_limite", () => {
  const ok = parseSessionInput(`resumen: a\ndecision: [${"a".repeat(40)}] x`);
  assert.equal(ok.decisions[0].slug.length, 40);
  assertInputError(`resumen: a\ndecision: [${"a".repeat(41)}] x`, /slug inválido/);
});

test("entrada_texto_limite", () => {
  const ok = parseSessionInput(`resumen: a\ndecision: [a] ${"x".repeat(200)}`);
  assert.equal(ok.decisions[0].text.length, 200);
  assertInputError(`resumen: a\ndecision: [a] ${"x".repeat(201)}`, /máximo 200/);
});

test("entrada_resumen_limite", () => {
  assert.equal(parseSessionInput(`resumen: ${"x".repeat(300)}`).summary.length, 300);
  assertInputError(`resumen: ${"x".repeat(301)}`, /máximo 300/);
});

test("entrada_texto_vacio", () => {
  assertInputError("resumen: a\ndecision: [a]   ", /texto vacío/);
});

test("entrada_slug_repetido", () => {
  assertInputError("resumen: a\ndecision: [a] x\ndecision: [a] y", /slug repetido 'a'/);
});

test("entrada_slug_igual_distinto_tipo", () => {
  const input = parseSessionInput("resumen: r\ndecision: [a] x\nerror: [a] y");
  assert.equal(input.decisions.length, 1);
  assert.equal(input.errors.length, 1);
});

test("entrada_demasiadas_decisiones", () => {
  const decisions = Array.from({ length: 11 }, (_, i) => `decision: [d${i}] x`);
  assertInputError(["resumen: a", ...decisions].join("\n"), /máximo 10/);
});

test("entrada_demasiados_errores", () => {
  const errors = Array.from({ length: 11 }, (_, i) => `error: [e${i}] x`);
  assertInputError(["resumen: a", ...errors].join("\n"), /máximo 10/);
});

test("entrada_con_secreto", () => {
  assertInputError(`resumen: usa ${hfToken}`, /posible secreto \(hugging-face\)/);
});

test("fecha_utc", () => {
  assert.equal(formatDate(new Date("2026-11-25T23:30:00-05:00")), "2026-11-26");
});

test("nombre_fichero_sesion", () => {
  const meta = { fecha: "2026-11-25", plan: "001-memoria", tarea: "T2", agente: "developer", n: 3 };
  assert.equal(sessionFileName(meta), "2026-11-25-001-memoria-T2-developer-3.md");
});

const reviewerMeta = {
  fecha: "2026-11-25",
  plan: "001-memoria",
  tarea: "T3",
  agente: "reviewer",
  agentId: "r1",
  n: 1,
  rama: "chore/001-memoria",
  gate: "verde",
  veredicto: "APPROVED",
  files: ["scripts/memory.mjs", "scripts/memory.test.mjs"],
};

const reviewerInput = {
  summary: "Revisa la CLI de session.",
  decisions: [
    {
      slug: "cli-sin-dependencias",
      text: "Parseo de argumentos a mano para no añadir dependencias.",
    },
  ],
  errors: [
    {
      slug: "validacion-args",
      text: "No se valida el formato de --task. #12",
      issues: [12],
    },
  ],
};

const reviewerGolden = `---
schema: memoria-sesion/1
fecha: 2026-11-25
plan: 001-memoria
tarea: T3
agente: reviewer
agent_id: r1
n: 1
rama: chore/001-memoria
gate: verde
veredicto: APPROVED
---

# Sesión 001-memoria · T3 · reviewer · 1

## Resumen

Revisa la CLI de session.

## Ficheros cambiados

- \`scripts/memory.mjs\`
- \`scripts/memory.test.mjs\`

## Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias.

## Errores

- **validacion-args** — No se valida el formato de --task. #12
`;

const emptyInput = { summary: "Nada.", decisions: [], errors: [] };

test("render_sesion_exacto", () => {
  assert.equal(renderSession(reviewerMeta, reviewerInput), reviewerGolden);
});

test("render_listas_vacias", () => {
  const md = renderSession({ ...reviewerMeta, files: [] }, emptyInput);
  assert.match(md, /## Ficheros cambiados\n\n- \(ninguno\)\n/);
  assert.match(md, /## Decisiones\n\n- \(ninguna\)\n/);
  assert.match(md, /## Errores\n\n- \(ninguno\)\n$/);
});

test("render_gate_rojo", () => {
  const meta = { ...reviewerMeta, gate: "rojo", gateFailed: ["Tests Rust", "Lint UI"] };
  assert.match(renderSession(meta, emptyInput), /^gate: rojo \(Tests Rust, Lint UI\)$/m);
});

test("render_developer_sin_veredicto", () => {
  const meta = { ...reviewerMeta, agente: "developer", agentId: "d1" };
  delete meta.veredicto;
  const md = renderSession(meta, emptyInput);
  assert.doesNotMatch(md, /veredicto:/);
});

test("parse_roundtrip", () => {
  const rendered = renderSession(reviewerMeta, reviewerInput);
  assert.deepEqual(parseSession(rendered), { meta: reviewerMeta, input: reviewerInput });
});

test("parse_roundtrip_gate_rojo_y_listas_vacias", () => {
  const meta = { ...reviewerMeta, gate: "rojo", gateFailed: ["Tests Rust", "Lint UI"], files: [] };
  delete meta.veredicto;
  assert.deepEqual(parseSession(renderSession(meta, emptyInput)), { meta, input: emptyInput });
});

test("parse_roundtrip_crlf", () => {
  const crlf = renderSession(reviewerMeta, reviewerInput).replace(/\n/g, "\r\n");
  assert.deepEqual(parseSession(crlf), { meta: reviewerMeta, input: reviewerInput });
});

test("parse_sin_front_matter", () => {
  assert.throws(() => parseSession("# hola"));
});

test("parse_schema_desconocido", () => {
  const md = renderSession(reviewerMeta, reviewerInput).replace("memoria-sesion/1", "memoria-sesion/2");
  assert.throws(() => parseSession(md), /schema/);
});

test("entrada_rechaza_resumen_con_almohadilla_inicial", () => {
  assertInputError("resumen: ## Errores corregidos", /línea 1.*'#'/);
});

test("entrada_error_empieza_por_issue", () => {
  const { errors } = parseSessionInput("resumen: a\nerror: [x] #12 falla");
  assert.deepEqual(errors, [{ slug: "x", text: "#12 falla", issues: [12] }]);
});

test("parse_roundtrip_error_y_decision_con_almohadilla_inicial", () => {
  const input = parseSessionInput("resumen: a\ndecision: [d] ## titulo\nerror: [x] #12 falla");
  assert.deepEqual(parseSession(renderSession(reviewerMeta, input)), { meta: reviewerMeta, input });
});

test("parse_roundtrip_resumen_con_almohadilla_interna", () => {
  const input = parseSessionInput("resumen: Cierra el issue #5 y ## no es cabecera");
  const meta = { ...reviewerMeta };
  assert.deepEqual(parseSession(renderSession(meta, input)), { meta, input });
});

test("parse_roundtrip_gate_rojo_sin_pasos", () => {
  const meta = { ...reviewerMeta, gate: "rojo", gateFailed: [] };
  assert.deepEqual(parseSession(renderSession(meta, emptyInput)), { meta, input: emptyInput });
});

test("entrada_slug_pegado_al_texto", () => {
  assertInputError("resumen: a\ndecision: [a]pegado", /línea 2.*slug/);
});

test("entrada_formato_estricto_clave_valor", () => {
  assertInputError("   resumen  : a", /línea 1/);
  assertInputError("resumen  : a", /línea 1/);
  assertInputError("resumen:a", /línea 1/);
});
