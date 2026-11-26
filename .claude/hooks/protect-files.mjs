// PreToolUse (Edit | Write | NotebookEdit | Bash | PowerShell):
// impide modificar los ficheros de governance (R8) y escribir la memoria fuera de scripts/memory.mjs (R11).

import { readHookInput, block, isProtected, PROTECTED_PATHS, PROJECT_DIR } from "./lib.mjs";

const input = readHookInput();
const toolInput = input.tool_input ?? {};
// Import dinámico: si el módulo faltara, R8 sigue aplicándose.
const policy = await import("../../scripts/lib/memory-policy.mjs").catch(() => null);

const filePath = toolInput.file_path ?? toolInput.notebook_path;
if (filePath && isProtected(filePath)) {
  block(`'${filePath}' es un fichero de governance y solo lo modifica el usuario. R8.`);
}

const rawCommand = toolInput.command;
if (rawCommand) {
  // El cuerpo de un heredoc de memory.mjs son datos, no órdenes: R8 solo evalúa la línea de la orden.
  const command = policy?.memoryCommandLine(rawCommand) ?? rawCommand;
  const writes = /(>|\bsed\s+-i\b|\brm\b|\bmv\b|\bcp\b|\btee\b|Set-Content|Add-Content|Out-File|Remove-Item|Move-Item|Copy-Item|New-Item)/i;
  const normalized = command.replace(/\\/g, "/");
  const touchesProtected = PROTECTED_PATHS.some((re) =>
    new RegExp(re.source.replace(/^\^/, "(^|[\\s\"'/])"), "i").test(normalized),
  );
  if (writes.test(command) && touchesProtected) {
    block("el comando modifica ficheros de governance, que solo modifica el usuario. R8.");
  }
}

if (policy) {
  const memory = policy.checkMemoryToolUse(input, { root: PROJECT_DIR });
  if (!memory.allow) block(memory.reason);
}

process.exit(0);
