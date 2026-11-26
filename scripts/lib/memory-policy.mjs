import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { checkInvocation, parseInvocation } from "./memory-cli-form.mjs";
import { tags } from "./memory-check.mjs";
import { runsInlineCode } from "./memory-inline.mjs";
import { isMemoryPath } from "./memory-path.mjs";
import { isAllowedRead } from "./memory-readonly.mjs";
import { startContextFor } from "./memory-start.mjs";
import { stopDecisionFor } from "./memory-stop.mjs";
import { stripDataHeredocs } from "./shell-heredoc.mjs";
import { mentionsCli, touchesMemory } from "./memory-touch.mjs";

const FILE_TOOLS = ["Edit", "Write", "NotebookEdit"];
const SHELL_TOOLS = ["Bash", "PowerShell"];
const ALLOW = { allow: true };
const ONLY_VIA_CLI = "R11: la memoria solo se escribe con node scripts/memory.mjs";
const UNSUPPORTED_FORM = "R11: forma de comando no admitida; usa exactamente node scripts/memory.mjs <subcomando>";
const NO_INLINE_CODE = "R11: no se admite código en línea en comandos que tocan la memoria";

const deny = (reason) => ({ allow: false, reason });

function checkFileTool({ tool_input: toolInput = {} }, root) {
  const filePath = toolInput.file_path ?? toolInput.notebook_path;
  return filePath && isMemoryPath(root, filePath) ? deny(ONLY_VIA_CLI) : ALLOW;
}

// Lista blanca: un comando que toca la memoria solo pasa si es la CLI exacta o una lectura permitida.
function checkMemoryTouchingCommand(command) {
  if (runsInlineCode(command)) return deny(NO_INLINE_CODE);
  if (isAllowedRead(command)) return ALLOW;
  return deny(mentionsCli(command) ? UNSUPPORTED_FORM : ONLY_VIA_CLI);
}

function checkShellCommand(input) {
  const command = input.tool_input?.command ?? "";
  const invocation = parseInvocation(command);
  if (invocation) return checkInvocation(input, invocation);
  // El cuerpo de un heredoc de datos no se ejecuta: solo se evalúa el resto de la orden.
  const effective = input.tool_name === "Bash" ? stripDataHeredocs(command) : command;
  return touchesMemory(effective) ? checkMemoryTouchingCommand(effective) : ALLOW;
}

export function checkMemoryToolUse(input, { root }) {
  if (FILE_TOOLS.includes(input.tool_name)) return checkFileTool(input, root);
  if (SHELL_TOOLS.includes(input.tool_name)) return checkShellCommand(input);
  return ALLOW;
}

// Lectura estricta: un medio plazo ilegible se avisa en el contexto, no se oculta como "sin slugs".
function readTagLines(root) {
  try {
    return tags(root);
  } catch (error) {
    return [`(no se pudieron leer los slugs: ${error.message})`];
  }
}

const LONG_FILES = ["decisiones.md", "errores.md"];

function readLongFile(path) {
  try {
    return readFileSync(path, "utf8").replaceAll("\r\n", "\n").trimEnd();
  } catch (error) {
    return `(no se pudo leer ${path}: ${error.message})`;
  }
}

function readLongTexts(root) {
  return LONG_FILES.map((name) => join(root, "docs", "memory", "long", name))
    .filter((path) => existsSync(path))
    .map(readLongFile);
}

export function subagentStartContext(input, { root }) {
  return startContextFor(input, { readTagLines: () => readTagLines(root), readLongTexts: () => readLongTexts(root) });
}

export function subagentStopDecision(input, { root, branch }) {
  return stopDecisionFor(input, { root, branch });
}
