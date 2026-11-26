import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { bash, developer, reviewer, withRoot } from "./memory-policy-fixtures.mjs";
import { renderSession } from "./memory-entry.mjs";
import { renderPlan } from "./memory-plan.mjs";
import { checkMemoryToolUse, subagentStartContext, subagentStopDecision } from "./memory-policy.mjs";


test("politica_bloquea_write_memoria", () =>
  withRoot((root) => {
    const input = { ...developer, tool_name: "Write", tool_input: { file_path: join(root, "docs", "memory", "short", "x.md") } };
    const result = checkMemoryToolUse(input, { root });
    assert.equal(result.allow, false);
    assert.match(result.reason, /R11/);
    assert.match(result.reason, /scripts\/memory\.mjs/);
  }));

test("politica_bloquea_edit_ruta_windows", () =>
  withRoot((root) => {
    const input = { ...developer, tool_name: "Edit", tool_input: { file_path: root + "\\DOCS\\Memory\\medium\\001-x.md" } };
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));

test("politica_bloquea_notebook", () =>
  withRoot((root) => {
    const input = { ...developer, tool_name: "NotebookEdit", tool_input: { notebook_path: join(root, "docs", "memory", "a.ipynb") } };
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));

test("politica_bloquea_orquestador", () =>
  withRoot((root) => {
    const input = { tool_name: "Edit", tool_input: { file_path: join(root, "docs", "memory", "long", "errores.md") } };
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));

test("politica_permite_fuera", () =>
  withRoot((root) => {
    const input = { ...developer, tool_name: "Write", tool_input: { file_path: join(root, "docs", "plans", "002-x.md") } };
    assert.deepEqual(checkMemoryToolUse(input, { root }), { allow: true });
  }));

test("politica_no_confunde_prefijo", () =>
  withRoot((root) => {
    const input = { ...developer, tool_name: "Write", tool_input: { file_path: join(root, "docs", "memoryx", "a.md") } };
    assert.deepEqual(checkMemoryToolUse(input, { root }), { allow: true });
  }));

test("politica_bash_escribe_memoria", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("echo x > docs/memory/short/a.md"), { root }).allow, false);
  }));

const sessionCommand = (agent, id, extra = "") =>
  `node scripts/memory.mjs session --agent ${agent} --agent-id ${id} --task T2${extra} <<'EOF'\nresumen: a -> b > c\nEOF`;

test("politica_session_developer", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(sessionCommand("developer", "a1")), { root }).allow, true);
  }));

test("politica_session_agente_falso", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(sessionCommand("reviewer", "a1", " --veredicto APPROVED")), { root }).allow, false);
  }));

test("politica_session_reviewer", () =>
  withRoot((root) => {
    const command = sessionCommand("reviewer", "r1", " --veredicto APPROVED");
    assert.equal(checkMemoryToolUse(bash(command, reviewer), { root }).allow, true);
  }));

test("politica_session_id_falso", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(sessionCommand("developer", "a2")), { root }).allow, false);
  }));

test("politica_session_planner", () =>
  withRoot((root) => {
    const planner = { agent_type: "planner", agent_id: "p1" };
    assert.equal(checkMemoryToolUse(bash(sessionCommand("planner", "p1"), planner), { root }).allow, false);
  }));

test("politica_session_orquestador", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(sessionCommand("developer", "a1"), {}), { root }).allow, false);
  }));

const FLUSH = "node scripts/memory.mjs flush --task T2 --agent-id r1";

test("politica_flush_reviewer", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(FLUSH, reviewer), { root }).allow, true);
  }));

test("politica_flush_id_falso", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(FLUSH, { agent_type: "reviewer", agent_id: "r2" }), { root }).allow, false);
  }));

test("politica_flush_developer", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(FLUSH, developer), { root }).allow, false);
  }));

const CLOSE = "node scripts/memory.mjs close";

test("politica_close_reviewer", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(CLOSE, reviewer), { root }).allow, true);
  }));

test("politica_close_developer", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash(CLOSE, developer), { root }).allow, false);
  }));

test("politica_promote_orquestador", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("node scripts/memory.mjs promote", {}), { root }).allow, false);
  }));

test("politica_lectura_cualquiera", () =>
  withRoot((root) => {
    for (const sub of ["check", "tags", "help"]) {
      assert.equal(checkMemoryToolUse(bash(`node scripts/memory.mjs ${sub}`, {}), { root }).allow, true, sub);
    }
  }));

test("politica_encadenado", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("node scripts/memory.mjs check && rm -rf docs/memory"), { root }).allow, false);
  }));

test("politica_redireccion", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("node scripts/memory.mjs tags > docs/memory/long/x.md"), { root }).allow, false);
  }));

test("politica_eof_intermedio", () =>
  withRoot((root) => {
    const command = "node scripts/memory.mjs session --agent developer --agent-id a1 --task T2 <<'EOF'\nEOF\nrm -rf docs/memory\nEOF";
    assert.equal(checkMemoryToolUse(bash(command), { root }).allow, false);
  }));

test("politica_heredoc_sin_cierre", () =>
  withRoot((root) => {
    const command = "node scripts/memory.mjs session --agent developer --agent-id a1 --task T2 <<'EOF'\nresumen: a";
    assert.equal(checkMemoryToolUse(bash(command), { root }).allow, false);
  }));

test("politica_heredoc_fuera_de_session", () =>
  withRoot((root) => {
    const command = `${FLUSH} <<'EOF'\nx\nEOF`;
    assert.equal(checkMemoryToolUse(bash(command, reviewer), { root }).allow, false);
  }));

test("politica_ruta_alternativa", () =>
  withRoot((root) => {
    for (const command of ["node ./scripts/memory.mjs check", String.raw`node scripts\memory.mjs check`]) {
      assert.equal(checkMemoryToolUse(bash(command, {}), { root }).allow, false, command);
    }
  }));

test("politica_import_directo", () =>
  withRoot((root) => {
    const command = `node -e "import('./scripts/lib/memory-plan.mjs')"`;
    assert.equal(checkMemoryToolUse(bash(command), { root }).allow, false);
  }));

test("politica_argumento_entrecomillado", () =>
  withRoot((root) => {
    const command = `node scripts/memory.mjs flush --task "T2" --agent-id r1`;
    assert.equal(checkMemoryToolUse(bash(command, reviewer), { root }).allow, false);
  }));

test("politica_powershell_escritura", () =>
  withRoot((root) => {
    const result = checkMemoryToolUse(bash(sessionCommand("developer", "a1"), developer, "PowerShell"), { root });
    assert.equal(result.allow, false);
    assert.match(result.reason, /usa la herramienta Bash/);
  }));

test("politica_powershell_lectura", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("node scripts/memory.mjs check", {}, "PowerShell"), { root }).allow, true);
  }));

test("politica_bash_rm_windows", () =>
  withRoot((root) => {
    const command = String.raw`Remove-Item docs\memory\long\errores.md`;
    assert.equal(checkMemoryToolUse(bash(command, developer, "PowerShell"), { root }).allow, false);
  }));

test("politica_bash_git_restore", () =>
  withRoot((root) => {
    for (const command of ["git checkout -- docs/memory", "git restore docs/memory/long"]) {
      assert.equal(checkMemoryToolUse(bash(command), { root }).allow, false, command);
    }
  }));

test("politica_bash_lee_memoria", () =>
  withRoot((root) => {
    for (const command of ["cat docs/memory/long/errores.md", "cat scripts/memory.mjs"]) {
      assert.equal(checkMemoryToolUse(bash(command), { root }).allow, true, command);
    }
  }));

test("politica_bash_ajeno", () =>
  withRoot((root) => {
    assert.equal(checkMemoryToolUse(bash("cargo test"), { root }).allow, true);
  }));

test("inicio_developer", () =>
  withRoot((root) => {
    const context = subagentStartContext(developer, { root });
    assert.match(context, /agent_id de esta sesión: a1/);
    assert.ok(context.includes("--agent developer --agent-id a1 --task <Tn> <<'EOF'"));
    assert.ok(!context.includes("flush"));
  }));

test("inicio_reviewer", () =>
  withRoot((root) => {
    const context = subagentStartContext(reviewer, { root });
    assert.ok(context.includes("--veredicto <APPROVED|CHANGES_REQUESTED>"));
    assert.ok(context.includes("flush --task <Tn> --agent-id r1"));
    assert.ok(context.includes("node scripts/memory.mjs close"));
  }));

test("inicio_tags_vacios", () =>
  withRoot((root) => {
    assert.match(subagentStartContext(developer, { root }), /\(ninguno\)/);
  }));

test("inicio_incluye_tags", () =>
  withRoot((root) => {
    const dir = join(root, "docs", "memory", "medium");
    mkdirSync(dir, { recursive: true });
    const doc = { plan: "001-x", estado: "cerrado", patterns: { decisions: [{ slug: "a", text: "t", tasks: ["T1"] }], errors: [] }, tasks: [] };
    writeFileSync(join(dir, "001-x.md"), renderPlan(doc));
    assert.match(subagentStartContext(developer, { root }), /decision a 1/);
  }));

test("inicio_planner_con_largo", () =>
  withRoot((root) => {
    const dir = join(root, "docs", "memory", "long");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "decisiones.md"), "# Decisiones recurrentes\n");
    writeFileSync(join(dir, "errores.md"), "# Errores recurrentes\n");
    const context = subagentStartContext({ agent_type: "planner", agent_id: "p1" }, { root });
    assert.ok(context.startsWith("[R11] Memoria a largo plazo"));
    assert.ok(context.includes("# Decisiones recurrentes") && context.includes("# Errores recurrentes"));
  }));

test("inicio_planner_vacio", () =>
  withRoot((root) => {
    assert.equal(subagentStartContext({ agent_type: "planner" }, { root }), "[R11] La memoria a largo plazo está vacía.");
  }));

test("inicio_otro_agente", () =>
  withRoot((root) => {
    assert.equal(subagentStartContext({ agent_type: "Explore", agent_id: "e1" }, { root }), null);
  }));

const BRANCH = "chore/001-memoria";
const EMPTY_INPUT = { summary: "a", decisions: [], errors: [] };

function sessionMeta(overrides) {
  return {
    fecha: "2026-11-25", plan: "001-memoria", tarea: "T3", agente: "developer", agentId: "a1", n: 1,
    rama: BRANCH, gate: "verde", files: [], ...overrides,
  };
}

function putSession(root, overrides, content = EMPTY_INPUT) {
  const meta = sessionMeta(overrides);
  const dir = join(root, "docs", "memory", "short");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `${meta.fecha}-${meta.plan}-${meta.tarea}-${meta.agente}-${meta.n}-${meta.agentId}.md`), renderSession(meta, content));
}

const stop = (identity, extra = {}) => ({ hook_event_name: "SubagentStop", ...identity, ...extra });

test("parada_developer_con_sesion", () =>
  withRoot((root) => {
    putSession(root, {});
    assert.deepEqual(subagentStopDecision(stop(developer), { root, branch: BRANCH }), { block: false });
  }));

test("parada_developer_sin_sesion", () =>
  withRoot((root) => {
    const decision = subagentStopDecision(stop(developer), { root, branch: BRANCH });
    assert.equal(decision.block, true);
    assert.ok(decision.reason.includes("--agent developer --agent-id a1"));
  }));

test("parada_stop_hook_active", () =>
  withRoot((root) => {
    const decision = subagentStopDecision(stop(developer, { stop_hook_active: true }), { root, branch: BRANCH });
    assert.equal(decision.block, true);
  }));

test("parada_id_exacto", () =>
  withRoot((root) => {
    putSession(root, { agentId: "a10" });
    assert.equal(subagentStopDecision(stop(developer), { root, branch: BRANCH }).block, true);
  }));

test("parada_fichero_corrupto", () =>
  withRoot((root) => {
    putSession(root, {});
    writeFileSync(join(root, "docs", "memory", "short", "roto.md"), "no es una sesión");
    assert.deepEqual(subagentStopDecision(stop(developer), { root, branch: BRANCH }), { block: false });
  }));

test("parada_otro_agente", () =>
  withRoot((root) => {
    const planner = { agent_type: "planner", agent_id: "p1" };
    assert.deepEqual(subagentStopDecision(stop(planner), { root, branch: BRANCH }), { block: false });
  }));

test("parada_rama_sin_plan", () =>
  withRoot((root) => {
    assert.deepEqual(subagentStopDecision(stop(developer), { root, branch: "main" }), { block: false });
    assert.deepEqual(subagentStopDecision(stop(developer), { root, branch: null }), { block: false });
  }));

const reviewerMeta = { agente: "reviewer", agentId: "r1", veredicto: "APPROVED" };

function putMedium(root, task, reviewerId) {
  const cycle = { cycle: 1, fecha: "2026-11-25", developer: 1, reviewer: reviewerId, gate: "verde", veredicto: "APPROVED" };
  const doc = { plan: "001-memoria", estado: "abierto", tasks: [{ task, cycles: [cycle], summaries: [], decisions: [], errors: [] }] };
  const dir = join(root, "docs", "memory", "medium");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "001-memoria.md"), renderPlan(doc));
}

test("parada_reviewer_sin_sesion", () =>
  withRoot((root) => {
    const decision = subagentStopDecision(stop(reviewer), { root, branch: BRANCH });
    assert.equal(decision.block, true);
    assert.ok(decision.reason.includes("session --agent reviewer --agent-id r1"));
  }));

test("parada_reviewer_sin_volcar", () =>
  withRoot((root) => {
    putSession(root, reviewerMeta);
    const decision = subagentStopDecision(stop(reviewer), { root, branch: BRANCH });
    assert.equal(decision.block, true);
    assert.ok(decision.reason.includes("flush --task T3 --agent-id r1"));
  }));

test("parada_reviewer_volcado", () =>
  withRoot((root) => {
    putMedium(root, "T3", "r1");
    assert.deepEqual(subagentStopDecision(stop(reviewer), { root, branch: BRANCH }), { block: false });
  }));

test("parada_reviewer_quedan_de_la_tarea", () =>
  withRoot((root) => {
    putMedium(root, "T3", "r1");
    putSession(root, { tarea: "T3" });
    const decision = subagentStopDecision(stop(reviewer), { root, branch: BRANCH });
    assert.equal(decision.block, true);
    assert.ok(decision.reason.includes("flush --task T3 --agent-id r1"));
  }));

test("parada_reviewer_otras_tareas_no_importan", () =>
  withRoot((root) => {
    putMedium(root, "T3", "r1");
    putSession(root, { tarea: "T4" });
    assert.deepEqual(subagentStopDecision(stop(reviewer), { root, branch: BRANCH }), { block: false });
  }));

test("parada_medio_ilegible_bloquea_explicito", () =>
  withRoot((root) => {
    const dir = join(root, "docs", "memory", "medium");
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "001-memoria.md"), "roto");
    const decision = subagentStopDecision(stop(reviewer), { root, branch: BRANCH });
    assert.equal(decision.block, true);
    assert.match(decision.reason, /001-memoria\.md/);
  }));

test("parada_short_ilegible_bloquea_sin_lanzar", () =>
  withRoot((root) => {
    mkdirSync(join(root, "docs", "memory"), { recursive: true });
    writeFileSync(join(root, "docs", "memory", "short"), "no es un directorio");
    for (const identity of [developer, reviewer]) {
      const decision = subagentStopDecision(stop(identity), { root, branch: BRANCH });
      assert.equal(decision.block, true);
      assert.match(decision.reason, /^no se puede comprobar la memoria \(R11\): /);
    }
  }));
