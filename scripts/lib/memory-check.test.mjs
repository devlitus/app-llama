import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { check, tags } from "./memory-check.mjs";
import { promote } from "./memory-long.mjs";
import { renderPlan } from "./memory-plan.mjs";

const GITIGNORE = "target/\ndocs/memory/short/\n";

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    writeFileSync(join(root, ".gitignore"), GITIGNORE);
    body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function write(root, rel, content) {
  const path = join(root, ...rel.split("/"));
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

const cycle = (n, veredicto = "APPROVED") => ({
  cycle: n,
  fecha: "2026-11-25",
  developer: 1,
  reviewer: `r${n}`,
  gate: "verde",
  veredicto,
});

const entry = (slug, extra) => ({ slug, text: `texto ${slug}`, ...extra });

function openDoc(plan, { cycles = [cycle(1)], decisions = [], errors = [], task = "T2" } = {}) {
  const task1 = {
    task,
    cycles,
    summaries: [],
    decisions: decisions.map((slug) => entry(slug, { cycles: [1] })),
    errors: errors.map((slug) => entry(slug, { cycles: [1], issues: [] })),
  };
  return { plan, estado: "abierto", tasks: [task1] };
}

function closedDoc(plan, { cycles = [cycle(1)], decisions = [], errors = [] } = {}) {
  return {
    plan,
    estado: "cerrado",
    patterns: {
      decisions: decisions.map((slug) => entry(slug, { tasks: ["T2"] })),
      errors: errors.map((slug) => entry(slug, { tasks: ["T2"], issues: [] })),
    },
    tasks: [{ task: "T2", cycles }],
  };
}

const seedMedium = (root, doc) => write(root, `docs/memory/medium/${doc.plan}.md`, renderPlan(doc));

function seedCoherent(root) {
  seedMedium(root, closedDoc("001-a"));
  seedMedium(root, openDoc("002-b"));
  promote(root);
}

const issuesOf = (root) => check(root).issues;
const readRel = (root, rel) => readFileSync(join(root, ...rel.split("/")), "utf8");

test("check_sin_memoria", () =>
  withRoot((root) => {
    assert.deepEqual(check(root), { issues: [], plans: 0, closed: 0 });
  }));

test("check_coherente", () =>
  withRoot((root) => {
    seedCoherent(root);
    assert.deepEqual(check(root), { issues: [], plans: 2, closed: 1 });
  }));

test("check_ignora_short", () =>
  withRoot((root) => {
    seedCoherent(root);
    write(root, "docs/memory/short/roto.md", "no es una sesión");
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_no_canonico", () =>
  withRoot((root) => {
    seedCoherent(root);
    const rel = "docs/memory/medium/002-b.md";
    write(root, rel, readRel(root, rel).replace("# Memoria del plan", "#  Memoria del plan"));
    assert.deepEqual(issuesOf(root), [`${rel}: formato no canónico`]);
  }));

test("check_medio_ilegible", () =>
  withRoot((root) => {
    seedCoherent(root);
    write(root, "docs/memory/medium/002-b.md", "# sin front matter\n");
    assert.deepEqual(issuesOf(root), ["docs/memory/medium/002-b.md: medio plazo sin front matter"]);
  }));

test("check_nombre_de_medio_invalido_no_oculta_el_resto", () =>
  withRoot((root) => {
    write(root, "docs/memory/medium/Notas.md", "x");
    seedMedium(root, openDoc("001-a", { cycles: [cycle(1), cycle(3)] }));
    const [invalidName, gap, ...rest] = issuesOf(root);
    assert.match(invalidName, /^docs\/memory\/medium\/Notas\.md: nombre de medio plazo no válido/);
    assert.equal(gap, "docs/memory/medium/001-a.md: T2: ciclos no consecutivos (1, 3)");
    assert.deepEqual(rest, []);
  }));

test("check_planes_hechos_en_orden_numerico", () =>
  withRoot((root) => {
    for (const plan of ["010-b", "002-a"]) {
      write(root, `docs/plans/${plan}.md`, "- **Estado:** hecho\n");
      seedMedium(root, openDoc(plan));
    }
    assert.deepEqual(issuesOf(root).map((issue) => issue.split(":")[0]), ["docs/plans/002-a.md", "docs/plans/010-b.md"]);
  }));

test("check_nombre_de_medio_invalido", () =>
  withRoot((root) => {
    write(root, "docs/memory/medium/Notas.md", "x");
    const [issue, ...rest] = issuesOf(root);
    assert.match(issue, /^docs\/memory\/medium\/Notas\.md: nombre de medio plazo no válido/);
    assert.deepEqual(rest, []);
  }));

test("check_nombre_incoherente", () =>
  withRoot((root) => {
    write(root, "docs/memory/medium/002-x.md", renderPlan(openDoc("001-memoria")));
    assert.deepEqual(issuesOf(root), [
      "docs/memory/medium/002-x.md: el plan del front matter es '001-memoria', no '002-x'",
    ]);
  }));

test("check_ciclos_saltados", () =>
  withRoot((root) => {
    seedMedium(root, openDoc("001-a", { cycles: [cycle(1, "CHANGES_REQUESTED"), cycle(3)] }));
    assert.deepEqual(issuesOf(root), ["docs/memory/medium/001-a.md: T2: ciclos no consecutivos (1, 3)"]);
  }));

test("check_cerrado_sin_approved", () =>
  withRoot((root) => {
    seedMedium(root, closedDoc("001-a", { cycles: [cycle(1, "CHANGES_REQUESTED")] }));
    promote(root);
    assert.deepEqual(issuesOf(root), ["docs/memory/medium/001-a.md: T2: el último ciclo no es APPROVED en un plan cerrado"]);
  }));

test("check_abierto_con_ultimo_ciclo_pendiente_es_valido", () =>
  withRoot((root) => {
    seedMedium(root, openDoc("001-a", { cycles: [cycle(1, "CHANGES_REQUESTED")] }));
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_largo_editado", () =>
  withRoot((root) => {
    seedCoherent(root);
    const rel = "docs/memory/long/errores.md";
    write(root, rel, `${readRel(root, rel)}- **a-mano** — x (planes: 001-a)\n`);
    assert.deepEqual(issuesOf(root), [`${rel}: desactualizado; ejecuta node scripts/memory.mjs promote`]);
  }));

test("check_largo_ausente_con_entradas_pendientes", () =>
  withRoot((root) => {
    ["001-a", "002-b", "003-c"].forEach((plan) => seedMedium(root, closedDoc(plan, { decisions: ["x"] })));
    assert.deepEqual(issuesOf(root), [
      "docs/memory/long/decisiones.md: desactualizado; ejecuta node scripts/memory.mjs promote",
    ]);
  }));

test("check_largo_no_se_valida_si_el_medio_es_ilegible", () =>
  withRoot((root) => {
    seedCoherent(root);
    write(root, "docs/memory/medium/003-c.md", "# nada\n");
    write(root, "docs/memory/long/errores.md", "editado a mano\n");
    assert.equal(issuesOf(root).length, 1);
  }));

test("check_plan_hecho_abierto", () =>
  withRoot((root) => {
    write(root, "docs/plans/001-x.md", "# Plan\n\n- **Estado:** hecho\n");
    seedMedium(root, openDoc("001-x"));
    assert.deepEqual(issuesOf(root), [
      "docs/plans/001-x.md: plan hecho con el medio plazo abierto; ejecuta node scripts/memory.mjs close",
    ]);
  }));

test("check_plan_hecho_con_medio_cerrado", () =>
  withRoot((root) => {
    write(root, "docs/plans/001-a.md", "# Plan\r\n\r\n- **Estado:** hecho\r\n");
    seedCoherent(root);
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_plan_en_curso_con_medio_abierto", () =>
  withRoot((root) => {
    write(root, "docs/plans/001-x.md", "# Plan\n\n- **Estado:** aprobado\n");
    seedMedium(root, openDoc("001-x"));
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_plan_hecho_sin_medio", () =>
  withRoot((root) => {
    write(root, "docs/plans/000-governance.md", "# Plan\n\n- **Estado:** hecho\n");
    write(root, "docs/plans/_TEMPLATE.md", "# Plantilla\n");
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_gitignore_sin_la_linea", () =>
  withRoot((root) => {
    write(root, ".gitignore", "target/\n# docs/memory/short/\ndocs/memory/short/x\n");
    assert.deepEqual(issuesOf(root), [".gitignore: falta la línea docs/memory/short/"]);
  }));

test("check_gitignore_ausente", () =>
  withRoot((root) => {
    rmSync(join(root, ".gitignore"));
    assert.deepEqual(issuesOf(root), [".gitignore: falta la línea docs/memory/short/"]);
  }));

test("check_gitignore_con_crlf", () =>
  withRoot((root) => {
    write(root, ".gitignore", "target/\r\ndocs/memory/short/\r\n");
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_tolera_crlf", () =>
  withRoot((root) => {
    seedCoherent(root);
    for (const rel of ["docs/memory/medium/001-a.md", "docs/memory/medium/002-b.md", "docs/memory/long/decisiones.md", "docs/memory/long/errores.md"]) {
      write(root, rel, readRel(root, rel).replace(/\n/g, "\r\n"));
    }
    assert.deepEqual(issuesOf(root), []);
  }));

test("check_acumula_incidencias", () =>
  withRoot((root) => {
    write(root, ".gitignore", "");
    seedMedium(root, openDoc("001-a", { cycles: [cycle(1), cycle(3)] }));
    assert.equal(issuesOf(root).length, 2);
  }));

test("tags_orden", () =>
  withRoot((root) => {
    seedMedium(root, openDoc("001-a", { decisions: ["a", "b"], errors: ["c"] }));
    seedMedium(root, closedDoc("002-b", { decisions: ["a", "b", "z"] }));
    assert.deepEqual(tags(root), ["decision a 2", "decision b 2", "decision z 1", "error c 1"]);
  }));

test("tags_ordena_por_planes_descendente_y_despues_por_slug", () =>
  withRoot((root) => {
    seedMedium(root, openDoc("001-a", { decisions: ["a", "m"] }));
    seedMedium(root, closedDoc("002-b", { decisions: ["m", "z"] }));
    seedMedium(root, closedDoc("003-c", { decisions: ["m", "z"] }));
    assert.deepEqual(tags(root), ["decision m 3", "decision z 2", "decision a 1"]);
  }));

test("tags_abiertos_y_cerrados", () =>
  withRoot((root) => {
    seedMedium(root, openDoc("001-a", { decisions: ["a"] }));
    seedMedium(root, closedDoc("002-b", { decisions: ["a"] }));
    assert.deepEqual(tags(root), ["decision a 2"]);
  }));

test("tags_sin_memoria", () =>
  withRoot((root) => {
    assert.deepEqual(tags(root), []);
  }));
