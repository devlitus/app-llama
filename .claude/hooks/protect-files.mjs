// PreToolUse (Edit | Write | NotebookEdit | Bash | PowerShell):
// impide que los agentes modifiquen los ficheros de governance. R8.

import { readHookInput, block, isProtected, PROTECTED_PATHS } from "./lib.mjs";

const input = readHookInput();
const toolInput = input.tool_input ?? {};

const filePath = toolInput.file_path ?? toolInput.notebook_path;
if (filePath && isProtected(filePath)) {
  block(`'${filePath}' es un fichero de governance y solo lo modifica el usuario. R8.`);
}

const command = toolInput.command;
if (command) {
  const writes = /(>|\bsed\s+-i\b|\brm\b|\bmv\b|\bcp\b|\btee\b|Set-Content|Add-Content|Out-File|Remove-Item|Move-Item|Copy-Item|New-Item)/i;
  const normalized = command.replace(/\\/g, "/");
  const touchesProtected = PROTECTED_PATHS.some((re) =>
    new RegExp(re.source.replace(/^\^/, "(^|[\\s\"'/])"), "i").test(normalized),
  );
  if (writes.test(command) && touchesProtected) {
    block("el comando modifica ficheros de governance, que solo modifica el usuario. R8.");
  }
}

process.exit(0);
