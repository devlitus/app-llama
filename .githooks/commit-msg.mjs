// Valida mensajes de commit según R9 (Conventional Commits, sin líneas de atribución).
// Uso:
//   node .githooks/commit-msg.mjs <fichero-del-mensaje>   (git hook commit-msg)
//   node .githooks/commit-msg.mjs --range <base>..<head>  (CI)

import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const USAGE = "Uso: node .githooks/commit-msg.mjs <fichero-del-mensaje> | --range <base>..<head>";
const TYPES = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"];
const HEADER = new RegExp(`^(${TYPES.join("|")})(\\([a-z0-9][a-z0-9-]*\\))?!?: \\S.*$`);
const MAX_HEADER = 72;
// Solo los asuntos que genera git; un "Merge loquesea" escrito a mano no está exento.
const GIT_GENERATED = /^(Merge (branch|remote-tracking branch|tag|commit|pull request) .+|Revert ".+")$/;
const AUTOSQUASH = /^(fixup|squash|amend)! /;
const ATTRIBUTION = [
  /^\s*[a-z]+(-[a-z]+)*-by\s*:/im,
  /generated (with|by)\b/i,
  /noreply@anthropic\.com/i,
  /\u{1F916}/u,
];

function commentChar() {
  const res = spawnSync("git", ["config", "core.commentChar"], { encoding: "utf8" });
  const value = res.status === 0 ? res.stdout.trim() : "";
  return value.length === 1 ? value : "#";
}

// git entrega al hook el mensaje sin limpiar: con `commit -v` el diff va tras la línea de tijeras.
// `comment` es null en --range: el mensaje ya está guardado tal cual y no se limpia nada.
function beforeScissors(message, comment) {
  const lines = message.split(/\r?\n/);
  if (comment === null) return lines;
  const scissors = lines.findIndex((line) => line.startsWith(comment) && line.includes(" >8 "));
  return scissors === -1 ? lines : lines.slice(0, scissors);
}

function withoutComments(lines, comment) {
  return (comment === null ? lines : lines.filter((line) => !line.startsWith(comment))).join("\n").trim();
}

function formatErrors(lines) {
  const header = lines[0] ?? "";
  const errors = [];
  if (!HEADER.test(header)) {
    errors.push(`el asunto no sigue Conventional Commits: "${header}". Formato: tipo(ámbito)!: descripción; tipos: ${TYPES.join(", ")}.`);
  }
  if (header.length > MAX_HEADER) {
    errors.push(`el asunto tiene ${header.length} caracteres (máximo ${MAX_HEADER}).`);
  }
  if (/\.$/.test(header)) {
    errors.push("el asunto no debe terminar en punto.");
  }
  if (lines.length > 1 && lines[1].trim() !== "") {
    errors.push("deja una línea en blanco entre el asunto y el cuerpo.");
  }
  return errors;
}

function validate(message, { allowAutosquash, comment }) {
  const raw = beforeScissors(message, comment);
  const lines = withoutComments(raw, comment).split("\n");
  const header = lines[0] ?? "";

  const errors = [];
  if (AUTOSQUASH.test(header)) {
    if (!allowAutosquash) {
      errors.push(`commit de autosquash sin integrar: "${header}". Ejecuta git rebase --autosquash antes del PR.`);
    }
  } else if (!GIT_GENERATED.test(header)) {
    errors.push(...formatErrors(lines));
  }
  // La atribución se comprueba siempre (también en merges, reverts y autosquash) y sobre las
  // líneas de comentario: con -F/-m git las conserva en el mensaje guardado.
  if (ATTRIBUTION.some((re) => re.test(raw.join("\n")))) {
    errors.push("prohibidas las líneas de atribución (Co-Authored-By, Signed-off-by, Generated with…).");
  }
  return errors;
}

function messagesInRange(range) {
  const res = spawnSync("git", ["log", "--format=%H%x00%B%x1e", range], { encoding: "utf8" });
  if (res.status !== 0) {
    process.stderr.write(res.stderr);
    process.exit(1);
  }
  return res.stdout
    .split("\x1e")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [sha, body] = entry.split("\x00");
      return { id: sha.slice(0, 8), message: body ?? "" };
    });
}

function usage() {
  process.stderr.write(`${USAGE}\n`);
  process.exit(1);
}

const args = process.argv.slice(2);
const isRange = args[0] === "--range";
if ((isRange && !args[1]) || (!isRange && !args[0])) usage();

const commits = isRange
  ? messagesInRange(args[1])
  : [{ id: "mensaje", message: readFileSync(args[0], "utf8") }];
const comment = isRange ? null : commentChar();

let failed = false;
for (const { id, message } of commits) {
  for (const error of validate(message, { allowAutosquash: !isRange, comment })) {
    failed = true;
    process.stderr.write(`[R9] ${id}: ${error}\n`);
  }
}
if (failed) {
  process.stderr.write("Commit rechazado. Ver docs/GOVERNANCE.md (R9).\n");
  process.exit(1);
}
