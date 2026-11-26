// PreToolUse (Edit | Write | NotebookEdit | Bash | PowerShell):
// impide escribir la memoria fuera de scripts/memory.mjs (R11).

import { readHookInput, block, PROJECT_DIR } from "./lib.mjs";

const MEMORY_MENTION = /docs[\\/]+memory/i;

// Un hook que lanza una excepción sale con 1, que no bloquea: ante un fallo, se deniega.
function safely(check) {
  try {
    return check();
  } catch (error) {
    return { allow: false, reason: `no se pudo analizar la orden (${error.message}); ante la duda, se bloquea.` };
  }
}

// Reserva fail-closed si falta el módulo de política: todo lo que mencione la memoria se bloquea.
function mentionsMemory(toolInput) {
  const target = toolInput.file_path ?? toolInput.notebook_path ?? toolInput.command ?? "";
  return MEMORY_MENTION.test(target);
}

const input = readHookInput();
const policy = await import("../../scripts/lib/memory-policy.mjs").catch(() => null);

const verdict = policy
  ? safely(() => policy.checkMemoryToolUse(input, { root: PROJECT_DIR }))
  : { allow: !mentionsMemory(input.tool_input ?? {}), reason: "R11: la memoria solo se escribe con node scripts/memory.mjs" };

if (!verdict.allow) block(verdict.reason);

process.exit(0);
