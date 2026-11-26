import { READ_ONLY, parseInvocation } from "./memory-cli-form.mjs";
import { splitCommandList, splitPipeline, wordsOf } from "./memory-shell-syntax.mjs";

const PLAIN_READS = new Set([
  "cat", "head", "tail", "wc", "diff", "grep", "egrep", "fgrep",
  "ls", "dir", "get-childitem", "gci", "test-path",
  "get-content", "gc", "type", "select-string", "sls", "measure-object", "select-object",
]);
const FORBIDDEN_OPTIONS = {
  rg: ["--pre"],
  find: ["-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls"],
};
const GIT_READS = new Set(["diff", "log", "show", "status", "blame", "ls-files", "grep"]);
const GIT_FORBIDDEN_OPTIONS = ["--output", "--ext-diff", "--open-files-in-pager"];
// `-O` admite el programa pegado (`-O/bin/rm`, `-Orm -v`): se prohíbe en cualquier posición de un argumento corto.
const GIT_FORBIDDEN_SHORT = /^-(?!-).*O/;
// `git add` solo escribe el índice; `-f` metería `short/` (D1) y `-p`, `-i` y `-e` abren un editor.
const GIT_ADD_FORBIDDEN_OPTIONS = ["--patch", "--interactive", "--edit", "--force"];
const GIT_ADD_FORBIDDEN_SHORT = /^-[a-z]*[pief][a-z]*$/;
// Redirecciones sin efecto (`2>&1`, `>/dev/null`): se quitan antes de validar la sintaxis.
const HARMLESS_REDIRECT = /(^|\s)[12]?>(&[12]|\/dev\/null)(?=\s|$)/g;
const MAX_CD_WORDS = 2;
const MIN_ABBREVIATION = 3;
const NODE_TEST_OPTIONS = ["--test-only", "--test-reporter=spec", "--test-reporter=tap", "--test-reporter=dot"];
const NODE_TEST_FILE = /^scripts\/.*\.test\.mjs$/;

// `--opción=valor` cuenta como la opción sola.
const optionName = (word) => word.split("=")[0];
const hasAny = (words, forbidden) => words.some((word) => forbidden.includes(optionName(word)));

// Git acepta abreviaturas de las opciones largas (`--open` es `--open-files-in-pager`).
function isForbiddenLongOption(word, forbidden) {
  const name = optionName(word);
  return name.startsWith("--") && name.length >= 2 + MIN_ABBREVIATION && forbidden.some((option) => option.startsWith(name));
}

const hasAbbreviated = (words, forbidden) => words.some((word) => isForbiddenLongOption(word, forbidden));

function isAllowedGit([, subcommand = "", ...rest]) {
  const name = subcommand.toLowerCase();
  if (name === "add") {
    return !hasAbbreviated(rest, GIT_ADD_FORBIDDEN_OPTIONS) && !rest.some((word) => GIT_ADD_FORBIDDEN_SHORT.test(word));
  }
  return GIT_READS.has(name) && !hasAbbreviated(rest, GIT_FORBIDDEN_OPTIONS) && !rest.some((word) => GIT_FORBIDDEN_SHORT.test(word));
}

function isAllowedNodeTest([, flag, ...rest]) {
  return flag === "--test" && rest.every((word) => NODE_TEST_OPTIONS.includes(word) || NODE_TEST_FILE.test(word));
}

function isAllowedNode(words, segment) {
  return isAllowedNodeTest(words) || READ_ONLY.includes(parseInvocation(segment)?.subcommand);
}

function isAllowedSegment(segment) {
  const words = wordsOf(segment);
  const command = (words[0] ?? "").toLowerCase();
  if (command === "git") return isAllowedGit(words);
  if (command === "node") return isAllowedNode(words, segment);
  if (command === "cd") return words.length <= MAX_CD_WORDS;
  if (command === "pwd") return words.length === 1;
  if (Object.hasOwn(FORBIDDEN_OPTIONS, command)) return !hasAny(words.slice(1), FORBIDDEN_OPTIONS[command]);
  return PLAIN_READS.has(command);
}

// Un elemento de la lista: comando simple o tubería de lecturas, sin sustituciones.
function isAllowedItem(item) {
  const segments = splitPipeline(item.replace(HARMLESS_REDIRECT, "$1"));
  return segments !== null && segments.every(isAllowedSegment);
}

// Lectura de la lista cerrada: lecturas simples o tuberías de ellas, encadenadas con `;`, `&&`, `||` o saltos de línea.
export function isAllowedRead(command) {
  const items = splitCommandList(command)?.filter(Boolean);
  return items !== undefined && items.length > 0 && items.every(isAllowedItem);
}
