// PreToolUse (Bash | PowerShell): bloquea comandos que saltan la governance.

import { readHookInput, block, git, PROJECT_DIR } from "./lib.mjs";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const BRANCH = /^(feat|fix|chore)\/(\d{3})-[a-z0-9]+(-[a-z0-9]+)*$/;

function planFor(branch) {
  const slug = branch.slice(branch.indexOf("/") + 1);
  const dir = join(PROJECT_DIR, "docs", "plans");
  return existsSync(dir) && readdirSync(dir).includes(`${slug}.md`);
}

const input = readHookInput();
const command = input.tool_input?.command ?? "";
const isGit = (sub) => new RegExp(`\\bgit\\b[^;&|\\n]*\\b${sub}\\b`).test(command);

if (/--no-verify\b/.test(command) || (isGit("commit") && /\s-n\b/.test(command))) {
  block("prohibido saltarse los git hooks (--no-verify). R1.");
}

if (/\bcore\.hooksPath\b/.test(command) && !/core\.hooksPath\s+\.githooks\b/.test(command) && !/--get\b/.test(command)) {
  block("prohibido cambiar core.hooksPath. R1.");
}

// Import dinámico: si push-hook.mjs no carga, el resto de reglas de este hook sigue activo.
const pushHook = await import("../../scripts/lib/push-hook.mjs").catch(() => null);
if (pushHook === null) {
  const plain = command.replaceAll("\\", "");
  if (/git\b/i.test(plain) && /\bpush\b/i.test(plain)) {
    block("no se pudo cargar scripts/lib/push-hook.mjs; ante la duda, se bloquea todo push. R5.");
  }
} else {
  const verdict = await pushHook.pushVerdict(command, {
    tool: input.tool_name,
    loadGuard: () => import("../../scripts/lib/push-guard.mjs"),
    currentBranch: () => git(["branch", "--show-current"]),
  });
  if (!verdict.allow) block(verdict.reason);
}

if (isGit("commit")) {
  if (/[a-z]+(-[a-z]+)*-by\s*:|generated (with|by)\b|noreply@anthropic\.com|\u{1F916}/iu.test(command)) {
    block("prohibidas las líneas de atribución en los commits (Co-Authored-By, Signed-off-by, Generated with…). R9.");
  }
  const branch = git(["branch", "--show-current"]);
  if (branch === "main" || branch === "master") {
    block(`no se hacen commits en '${branch}'. Crea una rama feat/, fix/ o chore/. R5.`);
  }
  if (!BRANCH.test(branch ?? "")) {
    block(`la rama '${branch}' no sigue feat|fix|chore/NNN-slug. Cada feature va en su rama dedicada. R10.`);
  }
  if (!planFor(branch)) {
    block(`no existe docs/plans/${branch.slice(branch.indexOf("/") + 1)}.md: la rama debe corresponder a un plan. R10.`);
  }
  if (git(["config", "core.hooksPath"]) !== ".githooks") {
    block("el git hook pre-commit no está activo. Ejecuta: git config core.hooksPath .githooks");
  }
}

process.exit(0);
