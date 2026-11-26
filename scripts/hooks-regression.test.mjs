import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HOOKS_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", ".claude", "hooks");
const PROTECT_FILES = join(HOOKS_DIR, "protect-files.mjs");
const GUARD_SHELL = join(HOOKS_DIR, "guard-shell.mjs");

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

const powershellPayload = (command) => ({ tool_name: "PowerShell", tool_input: { command } });

test("hook_caso_1_push_a_rama", () =>
  withRoot((root) => {
    const command =
      "cd /w/app-llama && git log --oneline main..HEAD && git push -u origin chore/001-memoria 2>&1 | tail -5";
    const result = runHook(GUARD_SHELL, root, bashPayload(command));
    assert.equal(result.status, 0, result.stderr);
  }));

test("hook_push_main", () =>
  withRoot((root) => {
    const result = runHook(GUARD_SHELL, root, bashPayload("git push origin HEAD:main"));
    assert.equal(result.status, 2);
    assert.match(result.stderr, /main\/master/);
  }));

test("hook_push_forzado", () =>
  withRoot((root) => {
    const result = runHook(GUARD_SHELL, root, bashPayload("git push -uf origin feat/x"));
    assert.equal(result.status, 2);
    assert.match(result.stderr, /forzado/);
  }));

test("hook_push_implicito_sin_repo", () =>
  withRoot((root) => {
    const result = runHook(GUARD_SHELL, root, bashPayload("git push"));
    assert.equal(result.status, 2);
  }));

test("hook_push_powershell", () =>
  withRoot((root) => {
    const result = runHook(GUARD_SHELL, root, powershellPayload("git log main; git push origin feat/x"));
    assert.equal(result.status, 2);
  }));

// Réplica mínima del árbol con push-hook.mjs roto, para comprobar que el import dinámico no desprotege el hook.
function withBrokenPushHook(body) {
  return withRoot((root) => {
    mkdirSync(join(root, ".claude", "hooks"), { recursive: true });
    mkdirSync(join(root, "scripts", "lib"), { recursive: true });
    for (const file of ["guard-shell.mjs", "lib.mjs"]) copyFileSync(join(HOOKS_DIR, file), join(root, ".claude", "hooks", file));
    writeFileSync(join(root, "scripts", "lib", "push-hook.mjs"), "export const = ;\n");
    return body(root, join(root, ".claude", "hooks", "guard-shell.mjs"));
  });
}

test("hook_push_modulo_roto_bloquea_el_push", () =>
  withBrokenPushHook((root, hook) => {
    const result = runHook(hook, root, bashPayload("git push origin feat/x"));
    assert.equal(result.status, 2);
    assert.match(result.stderr, /push-hook/);
  }));

test("hook_push_modulo_roto_mantiene_el_resto", () =>
  withBrokenPushHook((root, hook) => {
    assert.equal(runHook(hook, root, bashPayload("git log --oneline -1")).status, 0);
    assert.equal(runHook(hook, root, bashPayload("git commit -n -m x")).status, 2);
  }));
