import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { close } from "./memory-close.mjs";
import { renderSession } from "./memory-entry.mjs";
import { PreconditionError } from "./memory-errors.mjs";
import { compactPlan, renderPlan } from "./memory-plan.mjs";

const PLAN = "001-memoria";
const MEMORY = ["docs", "memory"];
const isPrecondition = (pattern) => (error) => error instanceof PreconditionError && pattern.test(error.message);

const cycle = (n, veredicto) => ({ cycle: n, fecha: "2026-11-25", developer: 1, reviewer: `r${n}`, gate: "verde", veredicto });
const task = (name, veredictos, extra = {}) => ({
  task: name,
  cycles: veredictos.map((veredicto, i) => cycle(i + 1, veredicto)),
  summaries: [{ cycle: 1, role: "developer", text: "resumen" }],
  decisions: [{ slug: "d", text: "decisión", cycles: [1] }],
  errors: [],
  ...extra,
});
const APPROVED_PLAN = { plan: PLAN, estado: "abierto", tasks: [task("T1", ["APPROVED"]), task("T2", ["CHANGES_REQUESTED", "APPROVED"])] };

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const mediumFile = (root) => join(root, ...MEMORY, "medium", `${PLAN}.md`);
const longFile = (root, name) => join(root, ...MEMORY, "long", name);
const readMediumText = (root) => readFileSync(mediumFile(root), "utf8");

function seedMedium(root, text) {
  mkdirSync(join(root, ...MEMORY, "medium"), { recursive: true });
  writeFileSync(mediumFile(root), text);
}

function seedSession(root, plan) {
  const meta = {
    fecha: "2026-11-25", plan, tarea: "T3", agente: "developer", agentId: "a1", n: 1,
    rama: `chore/${plan}`, gate: "verde", files: [],
  };
  const input = { summary: "x", decisions: [], errors: [] };
  mkdirSync(join(root, ...MEMORY, "short"), { recursive: true });
  writeFileSync(join(root, ...MEMORY, "short", `2026-11-25-${plan}-T3-developer-1.md`), renderSession(meta, input));
}

test("close_compacta_y_promueve", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(APPROVED_PLAN));
    const result = close(root, PLAN);
    assert.equal(readMediumText(root), renderPlan(compactPlan(APPROVED_PLAN)));
    assert.equal(result.rel, "docs/memory/medium/001-memoria.md");
    assert.equal(result.compacted, true);
    assert.deepEqual(result.long.written, ["docs/memory/long/decisiones.md", "docs/memory/long/errores.md"]);
    assert.ok(existsSync(longFile(root, "decisiones.md")) && existsSync(longFile(root, "errores.md")));
  }));

test("close_tarea_sin_approved_no_cambia_nada", () =>
  withRoot((root) => {
    const open = renderPlan({ ...APPROVED_PLAN, tasks: [task("T1", ["APPROVED"]), task("T2", ["APPROVED", "CHANGES_REQUESTED"])] });
    seedMedium(root, open);
    assert.throws(() => close(root, PLAN), isPrecondition(/T2/));
    assert.equal(readMediumText(root), open);
    assert.equal(existsSync(join(root, ...MEMORY, "long")), false);
  }));

test("close_nombra_todas_las_tareas_sin_approved", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan({ ...APPROVED_PLAN, tasks: [task("T1", ["CHANGES_REQUESTED"]), task("T10", ["CHANGES_REQUESTED"])] }));
    assert.throws(() => close(root, PLAN), isPrecondition(/T1.*T10/));
  }));

test("close_sesiones_pendientes_del_plan_no_cambia_nada", () =>
  withRoot((root) => {
    const open = renderPlan(APPROVED_PLAN);
    seedMedium(root, open);
    seedSession(root, PLAN);
    assert.throws(() => close(root, PLAN), isPrecondition(/pendientes.*short/));
    assert.equal(readMediumText(root), open);
    assert.equal(existsSync(join(root, ...MEMORY, "long")), false);
  }));

test("close_ignora_sesiones_de_otros_planes", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(APPROVED_PLAN));
    seedSession(root, "002-otro");
    assert.equal(close(root, PLAN).compacted, true);
  }));

test("close_sesion_ilegible_es_precondicion", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(APPROVED_PLAN));
    mkdirSync(join(root, ...MEMORY, "short"), { recursive: true });
    writeFileSync(join(root, ...MEMORY, "short", "rota.md"), "no es una sesión");
    assert.throws(() => close(root, PLAN), isPrecondition(/ilegible/));
  }));

test("close_sin_medio_plazo", () =>
  withRoot((root) => {
    assert.throws(() => close(root, PLAN), isPrecondition(/no existe.*001-memoria\.md/));
    assert.equal(existsSync(join(root, ...MEMORY)), false);
  }));

test("close_medio_ilegible", () =>
  withRoot((root) => {
    seedMedium(root, `${renderPlan(APPROVED_PLAN)}\nnota suelta\n`);
    const before = readMediumText(root);
    assert.throws(() => close(root, PLAN), isPrecondition(/ilegible.*001-memoria\.md/));
    assert.equal(readMediumText(root), before);
  }));

test("close_idempotente_byte_a_byte", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(APPROVED_PLAN));
    close(root, PLAN);
    const snapshot = () => [readMediumText(root), readFileSync(longFile(root, "decisiones.md"), "utf8"), readFileSync(longFile(root, "errores.md"), "utf8")];
    const before = snapshot();
    assert.equal(close(root, PLAN).compacted, false);
    assert.deepEqual(snapshot(), before);
  }));

test("close_sobre_plan_cerrado_regenera_el_largo_plazo", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(compactPlan(APPROVED_PLAN)));
    close(root, PLAN);
    assert.ok(existsSync(longFile(root, "decisiones.md")));
  }));

test("close_escritura_fallida_del_medio_nada_cambia", () =>
  withRoot((root) => {
    const open = renderPlan(APPROVED_PLAN);
    seedMedium(root, open);
    mkdirSync(join(root, ...MEMORY, "medium", `.${PLAN}.tmp`));
    assert.throws(() => close(root, PLAN), isPrecondition(/no se puede escribir.*001-memoria\.md/));
    assert.equal(readMediumText(root), open);
    assert.equal(existsSync(join(root, ...MEMORY, "long")), false);
  }));

test("close_fallo_en_largo_plazo_se_recupera_repitiendo", () =>
  withRoot((root) => {
    seedMedium(root, renderPlan(APPROVED_PLAN));
    mkdirSync(join(root, ...MEMORY, "long", ".decisiones.tmp"), { recursive: true });
    assert.throws(() => close(root, PLAN), isPrecondition(/decisiones\.md/));
    assert.equal(readMediumText(root), renderPlan(compactPlan(APPROVED_PLAN)));
    rmSync(join(root, ...MEMORY, "long", ".decisiones.tmp"), { recursive: true });
    assert.equal(close(root, PLAN).compacted, false);
    assert.ok(existsSync(longFile(root, "decisiones.md")));
  }));
