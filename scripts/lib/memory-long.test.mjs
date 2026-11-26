import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promote } from "./memory-long.mjs";
import { PreconditionError } from "./memory-errors.mjs";
import { renderPlan } from "./memory-plan.mjs";

const LONG = ["docs", "memory", "long"];
const isPrecondition = (pattern) => (error) => error instanceof PreconditionError && pattern.test(error.message);

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function seedMedium(root, plan, { estado = "cerrado", decisions = [], errors = [] } = {}) {
  const toEntry = ([slug, text = `${slug} ${plan}`]) => ({ slug, text, tasks: ["T1"] });
  const patterns = { decisions: decisions.map(toEntry), errors: errors.map(toEntry) };
  const doc = estado === "cerrado" ? { plan, estado, patterns, tasks: [] } : { plan, estado, tasks: [] };
  mkdirSync(join(root, "docs", "memory", "medium"), { recursive: true });
  writeFileSync(join(root, "docs", "memory", "medium", `${plan}.md`), renderPlan(doc));
}

const readLong = (root, name) => readFileSync(join(root, ...LONG, name), "utf8");
const seedThree = (root, options) => ["001-a", "002-b", "003-c"].forEach((plan) => seedMedium(root, plan, options));

test("promote_escribe_ambos_ficheros_aunque_no_haya_medio_plazo", () =>
  withRoot((root) => {
    const result = promote(root);
    assert.deepEqual(result, { written: ["docs/memory/long/decisiones.md", "docs/memory/long/errores.md"], dropped: [] });
    assert.match(readLong(root, "decisiones.md"), /- \(ninguna\)\n$/);
    assert.match(readLong(root, "errores.md"), /- \(ninguno\)\n$/);
    ["decisiones.md", "errores.md"].forEach((name) => assert.equal(readLong(root, name).split("\n").length - 1, 10));
  }));

test("promote_umbral_tres", () =>
  withRoot((root) => {
    seedThree(root, { decisions: [["a"]] });
    seedMedium(root, "004-d", { decisions: [["b"]] });
    seedMedium(root, "005-e", { decisions: [["b"]] });
    promote(root);
    const md = readLong(root, "decisiones.md");
    assert.match(md, /^- \*\*a\*\* — /m);
    assert.doesNotMatch(md, /\*\*b\*\*/);
  }));

test("promote_ignora_abiertos", () =>
  withRoot((root) => {
    seedMedium(root, "001-a", { decisions: [["a"]] });
    seedMedium(root, "002-b", { decisions: [["a"]] });
    seedMedium(root, "003-c", { estado: "abierto" });
    promote(root);
    assert.match(readLong(root, "decisiones.md"), /- \(ninguna\)\n$/);
  }));

test("promote_separa_tipos", () =>
  withRoot((root) => {
    seedMedium(root, "001-a", { decisions: [["x"]] });
    seedMedium(root, "002-b", { decisions: [["x"]], errors: [["x"]] });
    seedMedium(root, "003-c", { decisions: [["x"]], errors: [["x"]] });
    promote(root);
    assert.match(readLong(root, "decisiones.md"), /\*\*x\*\*/);
    assert.doesNotMatch(readLong(root, "errores.md"), /\*\*x\*\*/);
  }));

test("promote_texto_mas_reciente", () =>
  withRoot((root) => {
    seedMedium(root, "001-a", { decisions: [["a", "v1"]] });
    seedMedium(root, "004-d", { decisions: [["a", "v4"]] });
    seedMedium(root, "002-b", { decisions: [["a", "v2"]] });
    promote(root);
    assert.match(readLong(root, "decisiones.md"), /^- \*\*a\*\* — v4 \(planes: 001-a, 002-b, 004-d\)$/m);
  }));

test("promote_informa_de_lo_descartado", () =>
  withRoot((root) => {
    seedThree(root, { decisions: Array.from({ length: 41 }, (_, i) => [`s${String(i).padStart(2, "0")}`]) });
    assert.deepEqual(promote(root).dropped, [{ kind: "decision", slug: "s40" }]);
    assert.equal(readLong(root, "decisiones.md").split("\n").length - 1, 49);
  }));

test("promote_tamano_total", () =>
  withRoot((root) => {
    const slugs = Array.from({ length: 40 }, (_, i) => [`s${String(i).padStart(2, "0")}`]);
    seedThree(root, { decisions: slugs, errors: slugs });
    promote(root);
    const lines = ["decisiones.md", "errores.md"].map((name) => readLong(root, name).split("\n").length - 1);
    assert.deepEqual(lines, [49, 49]);
    assert.ok(lines[0] + lines[1] <= 100);
  }));

test("promote_idempotente_byte_a_byte", () =>
  withRoot((root) => {
    seedThree(root, { decisions: [["a"]], errors: [["e"]] });
    promote(root);
    const before = [readLong(root, "decisiones.md"), readLong(root, "errores.md")];
    promote(root);
    assert.deepEqual([readLong(root, "decisiones.md"), readLong(root, "errores.md")], before);
  }));

test("promote_medio_ilegible_no_escribe_nada", () =>
  withRoot((root) => {
    seedThree(root, { decisions: [["a"]] });
    writeFileSync(join(root, "docs", "memory", "medium", "004-d.md"), "# sin front matter\n");
    assert.throws(() => promote(root), isPrecondition(/ilegible.*004-d\.md/));
    assert.equal(existsSync(join(root, ...LONG)), false);
  }));

test("promote_escritura_fallida_no_deja_ficheros_a_medias", () =>
  withRoot((root) => {
    seedThree(root, { decisions: [["a"]] });
    mkdirSync(join(root, ...LONG, ".decisiones.tmp"), { recursive: true });
    assert.throws(() => promote(root), isPrecondition(/no se puede escribir docs\/memory\/long\/decisiones\.md/));
    assert.equal(existsSync(join(root, ...LONG, "decisiones.md")), false);
    assert.equal(existsSync(join(root, ...LONG, "errores.md")), false);
  }));

test("promote_repetido_tras_fallo_parcial_se_recupera", () =>
  withRoot((root) => {
    seedThree(root, { errors: [["e"]] });
    mkdirSync(join(root, ...LONG, ".errores.tmp"), { recursive: true });
    assert.throws(() => promote(root), isPrecondition(/errores\.md/));
    rmSync(join(root, ...LONG, ".errores.tmp"), { recursive: true });
    promote(root);
    assert.match(readLong(root, "errores.md"), /\*\*e\*\*/);
  }));
