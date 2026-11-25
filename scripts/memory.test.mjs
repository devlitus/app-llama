import { test } from "node:test";
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
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
    assert.equal(await run(["flush"], deps), 1);
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
