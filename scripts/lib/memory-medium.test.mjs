import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { listMediumPlans, mediumExists, readMedium, writeMedium } from "./memory-medium.mjs";
import { PreconditionError } from "./memory-errors.mjs";
import { renderPlan } from "./memory-plan.mjs";

const doc = (plan) => ({ plan, estado: "abierto", tasks: [] });
const EMPTY_CLOSED = { plan: "001-a", estado: "cerrado", patterns: { decisions: [], errors: [] }, tasks: [] };

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function seed(root, name, content) {
  mkdirSync(join(root, "docs", "memory", "medium"), { recursive: true });
  writeFileSync(join(root, "docs", "memory", "medium", name), content);
}

const isPrecondition = (pattern) => (error) => error instanceof PreconditionError && pattern.test(error.message);

test("medium_ausente_se_lee_como_abierto_vacio", () =>
  withRoot((root) => {
    assert.equal(mediumExists(root, "001-a"), false);
    assert.deepEqual(readMedium(root, "001-a"), { rel: "docs/memory/medium/001-a.md", doc: doc("001-a") });
  }));

test("medium_escribe_y_lee", () =>
  withRoot((root) => {
    writeMedium(root, "001-a", doc("001-a"));
    assert.equal(mediumExists(root, "001-a"), true);
    assert.deepEqual(readMedium(root, "001-a").doc, doc("001-a"));
  }));

test("medium_con_otro_plan_en_el_front_matter_es_ilegible", () =>
  withRoot((root) => {
    seed(root, "001-a.md", renderPlan(doc("002-b")));
    assert.throws(() => readMedium(root, "001-a"), isPrecondition(/ilegible.*001-a\.md.*002-b/));
  }));

test("medium_no_canonico_es_ilegible", () =>
  withRoot((root) => {
    seed(root, "001-a.md", `${renderPlan(doc("001-a"))}\nnota suelta\n`);
    assert.throws(() => readMedium(root, "001-a"), isPrecondition(/ilegible.*canónico/));
  }));

test("medium_lista_planes_en_orden_numerico_e_ignora_ocultos", () =>
  withRoot((root) => {
    for (const name of ["010-b.md", "002-a.md", ".002-a.tmp", "003-c.md"]) seed(root, name, "x");
    assert.deepEqual(listMediumPlans(root), ["002-a", "003-c", "010-b"]);
  }));

test("medium_lista_vacia_sin_directorio", () =>
  withRoot((root) => {
    assert.deepEqual(listMediumPlans(root), []);
  }));

test("medium_lista_con_nombre_invalido_es_ilegible", () =>
  withRoot((root) => {
    seed(root, "notas.md", "x");
    assert.throws(() => listMediumPlans(root), isPrecondition(/nombre.*notas\.md/));
  }));

test("medium_escritura_fallida_no_toca_el_original", () =>
  withRoot((root) => {
    writeMedium(root, "001-a", doc("001-a"));
    const before = readFileSync(join(root, "docs", "memory", "medium", "001-a.md"), "utf8");
    mkdirSync(join(root, "docs", "memory", "medium", ".001-a.tmp"));
    assert.throws(() => writeMedium(root, "001-a", EMPTY_CLOSED), isPrecondition(/no se puede escribir/));
    assert.equal(readFileSync(join(root, "docs", "memory", "medium", "001-a.md"), "utf8"), before);
  }));
