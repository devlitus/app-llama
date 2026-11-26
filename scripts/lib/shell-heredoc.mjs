// Despoja el cuerpo de los heredocs que Bash solo trata como datos (`cat <<'TAG'`).
// Ante cualquier duda devuelve la orden intacta: el llamador la analizará entera.
import { parseCommandList } from "./shell-words.mjs";

// El delimitador debe ir entre comillas (Bash no expande el cuerpo) y no ser un here-string `<<<`.
const HEADER = /(?<!<)<<(-?)(['"])([A-Za-z0-9_]+)\2/;
// Si la línea anterior no está cerrada, `cat` podría ser un argumento o parte de una tubería.
const OPEN_ENDING = /[|&\\]\s*$/;

function isBareCat(text) {
  const commands = parseCommandList(text);
  return commands?.length === 1 && commands[0].words.length === 1 && commands[0].words[0] === "cat";
}

// Devuelve la línea sin la cabecera, o null si no se puede garantizar que sea de datos.
function headerLineWithoutOperator(line, kept, header) {
  const rest = line.slice(header.index + header[0].length);
  const before = line.slice(0, header.index);
  // Con una tubería tras la cabecera, el cuerpo alimenta a la orden que sigue al terminador y esta sí se ejecuta.
  if (rest.includes("<<") || rest.includes("|")) return null;
  // Cada mitad debe cerrar sus propias comillas: si no, la cabecera estaría dentro de un texto entrecomillado.
  if (parseCommandList(before) === null || parseCommandList(rest) === null) return null;
  const stripped = `${before} ${rest}`;
  if (!isBareCat(stripped) || OPEN_ENDING.test(stripped)) return null;
  if (kept.length > 0 && OPEN_ENDING.test(kept[kept.length - 1])) return null;
  if (parseCommandList([...kept, stripped].join("\n")) === null) return null;
  return stripped;
}

function findTerminator(lines, from, tag, allowTabs) {
  const matches = (line) => (allowTabs ? line.replace(/^\t+/, "") : line) === tag;
  const offset = lines.slice(from).findIndex(matches);
  return offset === -1 ? -1 : from + offset;
}

/** Quita el cuerpo y el terminador de los heredocs de `cat` con delimitador entrecomillado. */
export function stripDataHeredocs(command) {
  if (command.includes("\r")) return command;
  const lines = command.split("\n");
  const kept = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.includes("<<")) {
      kept.push(line);
      i += 1;
      continue;
    }
    const header = HEADER.exec(line);
    if (header === null) return command;
    const stripped = headerLineWithoutOperator(line, kept, header);
    if (stripped === null) return command;
    const end = findTerminator(lines, i + 1, header[3], header[1] === "-");
    if (end === -1) return command;
    kept.push(stripped);
    i = end + 1;
  }
  return kept.join("\n");
}
