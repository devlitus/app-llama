import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HOOKS_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", ".claude", "hooks");
const PROTECT_FILES = join(HOOKS_DIR, "protect-files.mjs");

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "hooks-"));
  try {
    return body(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function runHook(hook, root, payload) {
  return spawnSync(process.execPath, [hook], {
    input: JSON.stringify({ cwd: root, ...payload }),
    encoding: "utf8",
    env: { ...process.env, CLAUDE_PROJECT_DIR: root, GIT_CEILING_DIRECTORIES: dirname(root) },
  });
}

const bashPayload = (command, identity = {}) => ({ ...identity, tool_name: "Bash", tool_input: { command } });
const writePayload = (file_path) => ({ tool_name: "Write", tool_input: { file_path, content: "x" } });

test("hook_write_en_hooks_permitido", () =>
  withRoot((root) => {
    const result = runHook(PROTECT_FILES, root, writePayload(join(root, ".claude", "hooks", "x.mjs")));
    assert.equal(result.status, 0, result.stderr);
  }));

test("hook_redireccion_a_governance_permitida", () =>
  withRoot((root) => {
    const command = "echo x > docs/GOVERNANCE.md && sed -i s/a/b/ scripts/lib/x.mjs";
    const result = runHook(PROTECT_FILES, root, bashPayload(command));
    assert.equal(result.status, 0, result.stderr);
  }));

test("hook_caso_4_lectura", () =>
  withRoot((root) => {
    const command =
      "ls docs/plans/ .claude/hooks/; git ls-tree --name-only origin/main .claude/hooks/; git fetch -q && git log --oneline -1 origin/main";
    for (const variant of [command, `${command} 2>/dev/null`]) {
      const result = runHook(PROTECT_FILES, root, bashPayload(variant));
      assert.equal(result.status, 0, result.stderr);
    }
  }));

test("hook_write_memoria_sigue_bloqueado", () =>
  withRoot((root) => {
    const result = runHook(PROTECT_FILES, root, writePayload(join(root, "docs", "memory", "short", "x.md")));
    assert.equal(result.status, 2);
    assert.match(result.stderr, /R11/);
  }));

test("hook_r11_rm", () =>
  withRoot((root) => {
    const result = runHook(PROTECT_FILES, root, bashPayload("rm -rf docs/memory/short"));
    assert.equal(result.status, 2);
    assert.match(result.stderr, /R11/);
  }));

test("hook_r11_sesion_permitida", () =>
  withRoot((root) => {
    const command =
      "node scripts/memory.mjs session --agent developer --agent-id a1 --task T1 <<'EOF'\nresumen: x\nEOF";
    const result = runHook(PROTECT_FILES, root, bashPayload(command, { agent_type: "developer", agent_id: "a1" }));
    assert.equal(result.status, 0, result.stderr);
  }));

test("hook_excepcion_bloquea", () =>
  withRoot((root) => {
    const result = runHook(PROTECT_FILES, root, bashPayload("constructor docs/memory"));
    assert.equal(result.status, 2);
  }));
