import { test } from "node:test";
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { failedGateSteps, run } from "./memory.mjs";
import { renderSession } from "./lib/memory-entry.mjs";

const SCRIPTS_DIR = dirname(fileURLToPath(import.meta.url));
const NOW = new Date("2026-11-25T10:00:00Z");
const SHORT = ["docs", "memory", "short"];
const SESSION_ARGS = ["session", "--agent", "developer", "--agent-id", "a1", "--task", "T2"];

function makeEnv({ branch = "chore/001-memoria", status = "", stdin = "resumen: a", gate = { ok: true, failed: [] }, planText } = {}) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  mkdirSync(join(root, "docs", "plans"), { recursive: true });
  writeFileSync(join(root, "docs", "plans", "001-memoria.md"), planText ?? "# Plan\n\n### T2 — x\n");
  const calls = { gate: 0 };
  const output = { out: "", err: "" };
  const deps = {
    root,
    now: () => NOW,
    git: (args) => (args.includes("status") ? status : args.includes("--show-current") ? `${branch}\n` : null),
    runGate: () => {
      calls.gate += 1;
      return gate;
    },
    readStdin: () => stdin,
    out: (text) => {
      output.out += `${text}\n`;
    },
    err: (text) => {
      output.err += `${text}\n`;
    },
  };
  return { root, deps, calls, output, cleanup: () => rmSync(root, { recursive: true, force: true }) };
}

async function withEnv(options, body) {
  const env = makeEnv(options);
  try {
    await body(env);
  } finally {
    env.cleanup();
  }
}

function shortDir(root) {
  return join(root, ...SHORT);
}

function shortFiles(root) {
  return existsSync(shortDir(root)) ? readdirSync(shortDir(root)).sort() : [];
}

function readShort(root, name) {
  return readFileSync(join(shortDir(root), name), "utf8");
}

function seedShort(root, name, content = "x") {
  mkdirSync(shortDir(root), { recursive: true });
  writeFileSync(join(shortDir(root), name), content);
}

const baseMeta = {
  fecha: "2026-11-25",
  plan: "001-memoria",
  tarea: "T2",
  agente: "developer",
  agentId: "a1",
  n: 1,
  rama: "chore/001-memoria",
  gate: "verde",
  files: [],
};

test("session_crea_fichero", () =>
  withEnv({}, async ({ root, deps, output }) => {
    assert.equal(await run(SESSION_ARGS, deps), 0);
    const name = "2026-11-25-001-memoria-T2-developer-1.md";
    const input = { summary: "a", decisions: [], errors: [] };
    assert.equal(readShort(root, name), renderSession(baseMeta, input));
    assert.equal(output.out.trim(), `docs/memory/short/${name}`);
  }));

test("session_incrementa_n", () =>
  withEnv({}, async ({ root, deps }) => {
    seedShort(root, "2026-11-25-001-memoria-T2-developer-1.md", "agent_id: otro\n");
    assert.equal(await run(SESSION_ARGS, deps), 0);
    assert.deepEqual(shortFiles(root), [
      "2026-11-25-001-memoria-T2-developer-1.md",
      "2026-11-25-001-memoria-T2-developer-2.md",
    ]);
  }));

test("session_n_por_agente", () =>
  withEnv({}, async ({ root, deps }) => {
    seedShort(root, "2026-11-25-001-memoria-T2-developer-1.md", "agent_id: otro\n");
    const args = ["session", "--agent", "reviewer", "--agent-id", "r1", "--task", "T2", "--veredicto", "APPROVED"];
    assert.equal(await run(args, deps), 0);
    assert.ok(shortFiles(root).includes("2026-11-25-001-memoria-T2-reviewer-1.md"));
  }));

test("session_ficheros_cambiados", () =>
  withEnv(
    { status: " M scripts/a.mjs\0?? docs/memory/short/x.md\0R  new.md\0old.md\0?? con espacio.md\0" },
    async ({ root, deps }) => {
      assert.equal(await run(SESSION_ARGS, deps), 0);
      const content = readShort(root, "2026-11-25-001-memoria-T2-developer-1.md");
      assert.match(content, /## Ficheros cambiados\n\n- `con espacio.md`\n- `new.md`\n- `scripts\/a.mjs`\n\n## Decisiones/);
    },
  ));

test("session_sin_cambios", () =>
  withEnv({ status: "" }, async ({ root, deps }) => {
    assert.equal(await run(SESSION_ARGS, deps), 0);
    assert.match(readShort(root, "2026-11-25-001-memoria-T2-developer-1.md"), /- \(ninguno\)/);
  }));

test("session_gate_rojo", () =>
  withEnv({ gate: { ok: false, failed: ["Tests Rust"] } }, async ({ root, deps }) => {
    assert.equal(await run(SESSION_ARGS, deps), 0);
    assert.match(readShort(root, "2026-11-25-001-memoria-T2-developer-1.md"), /^gate: rojo \(Tests Rust\)$/m);
  }));

test("session_reviewer_veredicto", () =>
  withEnv({}, async ({ root, deps }) => {
    const args = ["session", "--agent", "reviewer", "--agent-id", "r1", "--task", "T2", "--veredicto", "CHANGES_REQUESTED"];
    assert.equal(await run(args, deps), 0);
    assert.match(readShort(root, "2026-11-25-001-memoria-T2-reviewer-1.md"), /^veredicto: CHANGES_REQUESTED$/m);
  }));

test("session_rama_sin_plan", () =>
  withEnv({ branch: "main" }, async ({ root, deps, output }) => {
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /\[R11\] la rama 'main' no corresponde a un plan \(R10\)/);
    assert.deepEqual(shortFiles(root), []);
  }));

test("session_head_separado", () =>
  withEnv({ branch: "" }, async ({ deps }) => {
    assert.equal(await run(SESSION_ARGS, deps), 3);
  }));

test("session_plan_inexistente", () =>
  withEnv({ branch: "feat/002-x" }, async ({ deps, output }) => {
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /docs\/plans\/002-x\.md/);
  }));

test("session_tarea_inexistente", () =>
  withEnv({}, async ({ deps, output }) => {
    assert.equal(await run(["session", "--agent", "developer", "--agent-id", "a1", "--task", "T9"], deps), 3);
    assert.match(output.err, /no existe la tarea T9/);
  }));

test("session_tarea_no_confunde_prefijo", () =>
  withEnv({ planText: "### T21 — y\n" }, async ({ deps }) => {
    assert.equal(await run(SESSION_ARGS, deps), 3);
  }));

test("session_tarea_formato", () =>
  withEnv({}, async ({ deps }) => {
    for (const task of ["T2.1", "t2"]) {
      assert.equal(await run(["session", "--agent", "developer", "--agent-id", "a1", "--task", task], deps), 1);
    }
  }));

test("session_agente_invalido", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run(["session", "--agent", "planner", "--agent-id", "a1", "--task", "T2"], deps), 1);
  }));

test("session_agent_id_invalido", () =>
  withEnv({}, async ({ deps }) => {
    for (const id of ["a b", "x".repeat(65)]) {
      assert.equal(await run(["session", "--agent", "developer", "--agent-id", id, "--task", "T2"], deps), 1);
    }
  }));

test("session_falta_argumento", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run(["session", "--agent", "developer", "--task", "T2"], deps), 1);
  }));

test("session_reviewer_sin_veredicto", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run(["session", "--agent", "reviewer", "--agent-id", "r1", "--task", "T2"], deps), 1);
  }));

test("session_developer_con_veredicto", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run([...SESSION_ARGS, "--veredicto", "APPROVED"], deps), 1);
  }));

test("session_entrada_invalida_no_ejecuta_gate", () =>
  withEnv({ stdin: "" }, async ({ root, deps, calls, output }) => {
    assert.equal(await run(SESSION_ARGS, deps), 2);
    assert.match(output.err, /^\[R11\] /);
    assert.deepEqual(shortFiles(root), []);
    assert.equal(calls.gate, 0);
  }));

test("session_agent_id_duplicado", () =>
  withEnv({}, async ({ root, deps, calls }) => {
    seedShort(root, "2026-11-24-001-memoria-T1-developer-1.md", "---\nagent_id: a1\n---\n");
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.equal(shortFiles(root).length, 1);
    assert.equal(calls.gate, 0);
  }));

test("cli_sin_argumentos", () =>
  withEnv({}, async ({ deps, output }) => {
    assert.equal(await run([], deps), 1);
    assert.match(output.err, /session/);
  }));

test("cli_subcomando_desconocido", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run(["borrar"], deps), 1);
  }));

test("cli_subcomando_pendiente", () =>
  withEnv({}, async ({ deps, output }) => {
    assert.equal(await run(["close"], deps), 1);
    assert.match(output.err, /\[R11\] subcomando aún no disponible/);
  }));

test("cli_ayuda", () =>
  withEnv({}, async ({ deps, output }) => {
    for (const argv of [["--help"], ["help"]]) {
      output.out = "";
      assert.equal(await run(argv, deps), 0);
      for (const word of ["session", "flush", "close", "promote", "check", "tags"]) {
        assert.ok(output.out.includes(word), `falta ${word}`);
      }
    }
  }));

function makeRealRepo() {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  const git = (...args) => assert.equal(spawnSync("git", args, { cwd: root }).status, 0);
  git("init", "-q");
  git("symbolic-ref", "HEAD", "refs/heads/chore/001-x");
  mkdirSync(join(root, "docs", "plans"), { recursive: true });
  writeFileSync(join(root, "docs", "plans", "001-x.md"), "### T1 — a\n");
  mkdirSync(join(root, "scripts"), { recursive: true });
  cpSync(join(SCRIPTS_DIR, "memory.mjs"), join(root, "scripts", "memory.mjs"));
  cpSync(join(SCRIPTS_DIR, "lib"), join(root, "scripts", "lib"), {
    recursive: true,
    filter: (source) => !source.endsWith(".test.mjs"),
  });
  writeFileSync(
    join(root, "scripts", "quality-gate.mjs"),
    'process.stdout.write("──── Quality gate ────\\n✔ A\\n");\n',
  );
  return root;
}

function runSessionProcess(root, scriptPath) {
  return spawnSync(
    process.execPath,
    [scriptPath, "session", "--agent", "developer", "--agent-id", "a1", "--task", "T1"],
    { cwd: root, input: "resumen: a", encoding: "utf8" },
  );
}

function assertSessionWritten(root) {
  const files = readdirSync(join(root, ...SHORT));
  assert.equal(files.length, 1);
  assert.match(files[0], /^\d{4}-\d{2}-\d{2}-001-x-T1-developer-1\.md$/);
  const content = readFileSync(join(root, ...SHORT, files[0]), "utf8");
  assert.match(content, /^rama: chore\/001-x$/m);
  assert.match(content, /^gate: verde$/m);
}

test("cli_proceso_real", () => {
  const root = makeRealRepo();
  try {
    const result = runSessionProcess(root, "scripts/memory.mjs");
    assert.equal(result.status, 0, result.stderr);
    assertSessionWritten(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("cli_proceso_real_por_enlace", () => {
  const root = makeRealRepo();
  const link = join(mkdtempSync(join(tmpdir(), "mem-link-")), "enlace");
  try {
    symlinkSync(root, link, "junction");
    const result = runSessionProcess(root, join(link, "scripts", "memory.mjs"));
    assert.equal(result.status, 0, result.stderr);
    assertSessionWritten(root);
  } finally {
    rmSync(dirname(link), { recursive: true, force: true });
    rmSync(root, { recursive: true, force: true });
  }
});

test("session_plan_ilegible", () =>
  withEnv({}, async ({ root, deps, output }) => {
    const plan = join(root, "docs", "plans", "001-memoria.md");
    rmSync(plan);
    mkdirSync(plan);
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /^\[R11\] .*docs\/plans\/001-memoria\.md/);
  }));

test("session_sesion_ilegible_en_short", () =>
  withEnv({}, async ({ root, deps, output }) => {
    mkdirSync(join(shortDir(root), "x.md"), { recursive: true });
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /^\[R11\] .*x\.md/);
  }));

test("session_escritura_imposible", () =>
  withEnv({}, async ({ root, deps, output }) => {
    writeFileSync(join(root, "docs", "memory"), "ocupado");
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /^\[R11\] no se puede escribir docs\/memory\/short\/2026-11-25-001-memoria-T2-developer-1\.md/);
  }));

test("session_git_status_falla", () =>
  withEnv({}, async ({ root, deps, calls, output }) => {
    const git = deps.git;
    deps.git = (args) => (args.includes("status") ? null : git(args));
    assert.equal(await run(SESSION_ARGS, deps), 3);
    assert.match(output.err, /^\[R11\] .*git status/);
    assert.deepEqual(shortFiles(root), []);
    assert.equal(calls.gate, 0);
  }));

test("gate_rojo_extrae_pasos_fallidos", () => {
  const output = [
    "▶ Tests Rust",
    "✖ esto es salida del paso, no un resumen",
    "",
    "──── Quality gate ────",
    "✔ Formato Rust",
    "✖ Tests Rust",
    "✖ Lint UI",
    "✔ Patrones prohibidos",
    "",
    "QUALITY GATE: ROJO",
  ].join("\r\n");
  assert.deepEqual(failedGateSteps(output), ["Tests Rust", "Lint UI"]);
});

test("gate_verde_sin_pasos_fallidos", () => {
  assert.deepEqual(failedGateSteps("──── Quality gate ────\n✔ A\n"), []);
});

test("cli_importado_con_argv_inexistente_no_falla", () => {
  const moduleUrl = pathToFileURL(join(SCRIPTS_DIR, "memory.mjs")).href;
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "-e", `await import(${JSON.stringify(moduleUrl)})`, "ruta-inexistente"],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
});

const MEDIUM = ["docs", "memory", "medium", "001-memoria.md"];
const FLUSH_ARGS = (task, agentId) => ["flush", "--task", task, "--agent-id", agentId];

function seedSession(root, { agente, agentId, n = 1, fecha = "2026-11-25", tarea = "T3", veredicto, input = {} }) {
  const meta = { ...baseMeta, fecha, tarea, agente, agentId, n };
  if (veredicto) meta.veredicto = veredicto;
  const fullInput = { summary: `${agente} ${n}`, decisions: [], errors: [], ...input };
  const name = `${fecha}-001-memoria-${tarea}-${agente}-${n}.md`;
  seedShort(root, name, renderSession(meta, fullInput));
  return name;
}

const seedDeveloper = (root, options = {}) =>
  seedSession(root, { agente: "developer", agentId: `d${options.n ?? 1}`, ...options });
const seedReviewer = (root, options = {}) =>
  seedSession(root, { agente: "reviewer", agentId: "r1", veredicto: "APPROVED", ...options });

function readMedium(root) {
  return readFileSync(join(root, ...MEDIUM), "utf8");
}

const CYCLE_1_INPUT = {
  developer: {
    summary: "Implementa la CLI de session.",
    decisions: [{ slug: "cli-sin-dependencias", text: "Parseo de argumentos a mano para no añadir dependencias." }],
  },
  reviewer: {
    summary: "Falta validar el formato de --task.",
    errors: [{ slug: "validacion-args", text: "No se valida el formato de --task. #12", issues: [12] }],
  },
};

const MEDIUM_HEAD = `---
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
`;

const MEDIUM_CYCLE_2_ROW = "| 2 | 2026-11-26 | 1 | r2 | verde | APPROVED |\n";

const MEDIUM_CYCLE_1_BODY = `
### Resúmenes

- C1 · developer · Implementa la CLI de session.
- C1 · reviewer · Falta validar el formato de --task.
`;

const MEDIUM_CYCLE_2_SUMMARIES = `- C2 · developer · Valida --task con ^T\\d+$.
- C2 · reviewer · Revisa la corrección de --task.
`;

const MEDIUM_TAIL = `
### Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (C1)

### Errores

- **validacion-args** — No se valida el formato de --task. #12 (C1 · #12)
`;

function seedCycleOne(root) {
  seedDeveloper(root, { input: CYCLE_1_INPUT.developer });
  seedReviewer(root, { veredicto: "CHANGES_REQUESTED", input: CYCLE_1_INPUT.reviewer });
}

test("flush_crea_medio", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedCycleOne(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0, output.err);
    assert.equal(readMedium(root), MEDIUM_HEAD + MEDIUM_CYCLE_1_BODY + MEDIUM_TAIL);
    assert.deepEqual(shortFiles(root), []);
  }));

test("flush_informa_por_stdout", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedCycleOne(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.equal(output.out, "docs/memory/medium/001-memoria.md\nciclo: T3 C1\nborradas: 2\n");
  }));

test("flush_segundo_ciclo", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedCycleOne(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    seedDeveloper(root, { fecha: "2026-11-26", input: { summary: "Valida --task con ^T\\d+$." } });
    seedReviewer(root, {
      agentId: "r2",
      fecha: "2026-11-26",
      input: { summary: "Revisa la corrección de --task." },
    });
    assert.equal(await run(FLUSH_ARGS("T3", "r2"), deps), 0, output.err);
    const expected = MEDIUM_HEAD + MEDIUM_CYCLE_2_ROW + MEDIUM_CYCLE_1_BODY + MEDIUM_CYCLE_2_SUMMARIES + MEDIUM_TAIL;
    assert.equal(readMedium(root), expected);
    assert.deepEqual(shortFiles(root), []);
  }));

test("flush_acumula_slug", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root, {
      veredicto: "CHANGES_REQUESTED",
      input: { errors: [{ slug: "validacion-args", text: "falla #12", issues: [12] }] },
    });
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    seedReviewer(root, {
      agentId: "r2",
      input: { errors: [{ slug: "validacion-args", text: "texto nuevo #15", issues: [15] }] },
    });
    assert.equal(await run(FLUSH_ARGS("T3", "r2"), deps), 0);
    assert.match(readMedium(root), /^- \*\*validacion-args\*\* — texto nuevo #15 \(C1, C2 · #12, #15\)$/m);
  }));

test("flush_varios_developer", () =>
  withEnv({}, async ({ root, deps }) => {
    seedDeveloper(root, { n: 2 });
    seedDeveloper(root, { n: 10 });
    seedReviewer(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    const medium = readMedium(root);
    assert.match(medium, /^\| 1 \| 2026-11-25 \| 2 \| r1 \| verde \| APPROVED \|$/m);
    assert.match(medium, /- C1 · developer · developer 2\n- C1 · developer · developer 10\n- C1 · reviewer · reviewer 1\n/);
  }));

test("flush_ordena_developer_por_fecha_y_despues_por_n", () =>
  withEnv({}, async ({ root, deps }) => {
    seedDeveloper(root, { n: 1, fecha: "2026-11-26" });
    seedDeveloper(root, { n: 2, fecha: "2026-11-25" });
    seedReviewer(root, { fecha: "2026-11-26" });
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.match(readMedium(root), /- C1 · developer · developer 2\n- C1 · developer · developer 1\n/);
  }));

test("flush_sin_developer", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.match(readMedium(root), /^\| 1 \| 2026-11-25 \| 0 \| r1 \| verde \| APPROVED \|$/m);
  }));

test("flush_orden_tareas", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root, { tarea: "T10" });
    assert.equal(await run(FLUSH_ARGS("T10", "r1"), deps), 0);
    seedReviewer(root, { tarea: "T2", agentId: "r2" });
    assert.equal(await run(FLUSH_ARGS("T2", "r2"), deps), 0);
    assert.deepEqual(readMedium(root).match(/^## T\d+$/gm), ["## T2", "## T10"]);
  }));

test("flush_no_toca_otras_tareas", () =>
  withEnv({}, async ({ root, deps }) => {
    seedDeveloper(root, { tarea: "T3" });
    seedReviewer(root, { tarea: "T3" });
    const other = seedDeveloper(root, { tarea: "T4" });
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.deepEqual(shortFiles(root), [other]);
    assert.doesNotMatch(readMedium(root), /## T4/);
  }));

test("flush_sin_reviewer", () =>
  withEnv({}, async ({ root, deps, output }) => {
    const dev = seedDeveloper(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.match(output.err, /^\[R11\] /);
    assert.deepEqual(shortFiles(root), [dev]);
    assert.equal(existsSync(join(root, ...MEDIUM)), false);
  }));

test("flush_dos_reviewer", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root, { agentId: "r1" });
    seedReviewer(root, { agentId: "r2", n: 2 });
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.equal(shortFiles(root).length, 2);
  }));

test("flush_agent_id_distinto", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root, { agentId: "r1" });
    assert.equal(await run(FLUSH_ARGS("T3", "r9"), deps), 3);
    assert.equal(shortFiles(root).length, 1);
  }));

test("flush_plan_cerrado", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedReviewer(root);
    mkdirSync(dirname(join(root, ...MEDIUM)), { recursive: true });
    const closed = "---\nschema: memoria-plan/2\nplan: 001-memoria\nestado: cerrado\n---\n\n# Memoria del plan 001-memoria\n";
    writeFileSync(join(root, ...MEDIUM), closed);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.match(output.err, /cerrado/);
    assert.equal(readMedium(root), closed);
    assert.equal(shortFiles(root).length, 1);
  }));

test("flush_medio_ilegible", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedReviewer(root);
    mkdirSync(dirname(join(root, ...MEDIUM)), { recursive: true });
    writeFileSync(join(root, ...MEDIUM), "# sin front matter\n");
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.match(output.err, /docs\/memory\/medium\/001-memoria\.md/);
    assert.equal(shortFiles(root).length, 1);
  }));

test("flush_reintento_idempotente", () =>
  withEnv({}, async ({ root, deps, output }) => {
    seedCycleOne(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    const before = readMedium(root);
    seedDeveloper(root, { n: 2, input: { summary: "sobrante" } });
    seedReviewer(root, { veredicto: "CHANGES_REQUESTED", input: CYCLE_1_INPUT.reviewer });
    output.out = "";
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.equal(readMedium(root), before);
    assert.deepEqual(shortFiles(root), []);
    assert.match(output.out, /ciclo: T3 C1\nborradas: 2\n$/);
  }));

test("flush_rama_sin_plan", () =>
  withEnv({ branch: "main" }, async ({ root, deps }) => {
    seedReviewer(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.equal(shortFiles(root).length, 1);
  }));

test("flush_argumentos", () =>
  withEnv({}, async ({ deps }) => {
    assert.equal(await run(["flush", "--agent-id", "r1"], deps), 1);
    assert.equal(await run(["flush", "--task", "T3"], deps), 1);
    assert.equal(await run(["flush", "--task", "T3.1", "--agent-id", "r1"], deps), 1);
    assert.equal(await run(["flush", "--task", "T3", "--agent-id", "a b"], deps), 1);
  }));

test("flush_estado_desconocido", () =>
  withEnv({}, async ({ root, deps, output }) => {
    const session = seedReviewer(root);
    mkdirSync(dirname(join(root, ...MEDIUM)), { recursive: true });
    const odd = MEDIUM_HEAD.replace("estado: abierto", "estado: Abierto") + MEDIUM_CYCLE_1_BODY + MEDIUM_TAIL;
    writeFileSync(join(root, ...MEDIUM), odd);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.match(output.err, /ilegible.*docs\/memory\/medium\/001-memoria\.md/);
    assert.equal(readMedium(root), odd);
    assert.deepEqual(shortFiles(root), [session]);
  }));

test("flush_escritura_imposible", () =>
  withEnv({}, async ({ root, deps, output }) => {
    const session = seedReviewer(root);
    mkdirSync(join(root, "docs", "memory", "medium", ".001-memoria.tmp"), { recursive: true });
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 3);
    assert.match(output.err, /^\[R11\] no se puede escribir docs\/memory\/medium\/001-memoria\.md/);
    assert.deepEqual(shortFiles(root), [session]);
    assert.equal(existsSync(join(root, ...MEDIUM)), false);
  }));

test("flush_no_deja_temporales", () =>
  withEnv({}, async ({ root, deps }) => {
    seedReviewer(root);
    assert.equal(await run(FLUSH_ARGS("T3", "r1"), deps), 0);
    assert.deepEqual(readdirSync(join(root, "docs", "memory", "medium")), ["001-memoria.md"]);
  }));
