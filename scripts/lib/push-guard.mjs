// R5: bloquea el push forzado y el push a main/master analizando solo los argumentos del `push`.

import { parseCommandList } from "./shell-words.mjs";
import { stripDataHeredocs } from "./shell-heredoc.mjs";

const ALLOW = { allow: true };
const deny = (reason) => ({ allow: false, reason });

const FORCED = deny("prohibido el push forzado. R5.");
const PROTECTED = deny("prohibido hacer push directo a main/master; usa una rama y PR. R5.");
const OPAQUE = deny(
  "no se puede analizar el push con certeza (variables, sustituciones, -c o argumentos ocultos); escribe la orden con la rama literal. R5.",
);

const GIT_WORD = /^(?:.*[\\/])?git(?:\.exe)?$/i;
const GIT_PUSH_WORD = /^(?:.*[\\/])?git-push(?:\.exe)?$/i;
const BATCH_RUNNER = /^(?:.*[\\/])?(?:xargs|parallel)(?:\.exe)?$/i;
const ENV_CONFIG = /GIT_CONFIG_(?:COUNT|KEY_|VALUE_|PARAMETERS)/;
const MAX_NESTING = 3;

const GLOBAL_OPTIONS_WITH_VALUE = new Set(["-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env", "--attr-source"]);
const GLOBAL_OPTIONS_WITH_INLINE_VALUE = new Set(["--exec-path", "--super-prefix", "--list-cmds"]);
const GLOBAL_FLAGS = new Set([
  "-p", "-P", "-h", "-v", "--paginate", "--no-pager", "--bare", "--no-replace-objects", "--literal-pathspecs",
  "--glob-pathspecs", "--noglob-pathspecs", "--icase-pathspecs", "--no-optional-locks", "--no-lazy-fetch",
  "--no-advice", "--exec-path", "--html-path", "--man-path", "--info-path", "--version", "--help",
]);
// Un alias (de `-c` o de GIT_CONFIG_*) no puede sobrescribir un built-in; sí puede disfrazar un push.
const BUILTIN_SUBCOMMANDS = new Set([
  "add", "am", "apply", "bisect", "blame", "branch", "checkout", "cherry-pick", "clean", "clone", "commit",
  "config", "describe", "diff", "fetch", "grep", "init", "log", "ls-files", "ls-tree", "merge", "mv", "pull",
  "rebase", "reflog", "remote", "reset", "restore", "revert", "rev-parse", "rm", "shortlog", "show", "stash",
  "status", "submodule", "switch", "tag", "worktree",
]);

const FORCE_OPTIONS = ["--force", "--force-with-lease", "--force-if-includes"];
const MIN_FORCE_ABBREVIATION = "--for".length;
const WHOLE_REPOSITORY_OPTIONS = ["--all", "--mirror", "--branches"];
const LONG_OPTIONS_WITH_VALUE = ["--push-option", "--repo", "--receive-pack", "--exec"];
const CURRENT_BRANCH_REFS = new Set(["HEAD", "@"]);

// Regla anterior, para lo que no se puede analizar: busca en toda la orden, no en los argumentos.
// Sin `\b` antes de `git`: una opción pegada (`env -S'git push …'`) llega como `-Sgit push …`.
const LEGACY_PUSH = /git\b[^;&|\n]*\bpush\b[^;&|\n]*/gi;
const LEGACY_FORCE = /(--force\b|--force-with-lease\b|\s-f\b|\s\+\S)/;
const LEGACY_PROTECTED = /\b(main|master)\b/i;
const LEGACY_UNRESOLVED = /[$`]/;

export function legacyPushVerdict(command) {
  const plain = command.replaceAll("\\", "");
  const pushes = plain.match(LEGACY_PUSH);
  if (pushes === null) return ALLOW;
  if (LEGACY_FORCE.test(plain)) return FORCED;
  if (LEGACY_PROTECTED.test(plain)) return PROTECTED;
  if (pushes.some((push) => LEGACY_UNRESOLVED.test(push))) return OPAQUE;
  return ALLOW;
}

function isKnownGlobalOption(option) {
  const [name] = option.split("=");
  return GLOBAL_FLAGS.has(option) || GLOBAL_OPTIONS_WITH_VALUE.has(name) || GLOBAL_OPTIONS_WITH_INLINE_VALUE.has(name);
}

function readGlobalOptions(args) {
  let index = 0;
  let configured = false;
  let unknown = false;
  while (index < args.length && args[index].startsWith("-")) {
    const option = args[index];
    if (option === "-c" || option === "--config-env" || option.startsWith("--config-env=")) configured = true;
    if (!isKnownGlobalOption(option)) unknown = true;
    index += GLOBAL_OPTIONS_WITH_VALUE.has(option) ? 2 : 1;
  }
  return { configured, unknown, subcommand: args[index], rest: args.slice(index + 1) };
}

// git acepta cualquier prefijo no vacío de una opción larga; si es ambiguo, se toma la lectura más estricta.
const abbreviates = (name, option) => name.length > "--".length && option.startsWith(name);
const abbreviatesAny = (name, options) => options.some((option) => abbreviates(name, option));

// Devuelve cuántas palabras extra consume la opción como valor.
function readLongOption(arg, parsed) {
  const [name] = arg.split("=");
  if (name.length >= MIN_FORCE_ABBREVIATION && abbreviatesAny(name, FORCE_OPTIONS)) parsed.forced = true;
  if (abbreviatesAny(name, WHOLE_REPOSITORY_OPTIONS)) parsed.wholeRepository = true;
  return abbreviatesAny(name, LONG_OPTIONS_WITH_VALUE) && !arg.includes("=") ? 1 : 0;
}

function readShortOptions(arg, parsed) {
  const letters = arg.slice(1);
  for (let k = 0; k < letters.length; k += 1) {
    if (letters[k] === "f") parsed.forced = true;
    // `-o` toma el resto del grupo como valor, o la palabra siguiente si no hay resto.
    if (letters[k] === "o") return k === letters.length - 1 ? 1 : 0;
  }
  return 0;
}

function parsePushArguments(args) {
  const parsed = { forced: false, wholeRepository: false, positionals: [] };
  let optionsEnded = false;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (optionsEnded || !arg.startsWith("-")) parsed.positionals.push(arg);
    else if (arg === "--") optionsEnded = true;
    else i += arg.startsWith("--") ? readLongOption(arg, parsed) : readShortOptions(arg, parsed);
  }
  return parsed;
}

const withoutPlus = (refspec) => refspec.replace(/^\+/, "");

// git resuelve `heads/main` y `remotes/o/main` como `refs/heads/main` y `refs/remotes/o/main`.
function isProtectedRef(ref) {
  if (ref.includes("*")) return true;
  const name = ref
    .replace(/^refs\//, "")
    .replace(/^(?:heads\/|remotes\/[^/]+\/)/, "")
    .replace(/(?:[~^]\d*)+$/, "")
    .toLowerCase();
  return name === "main" || name === "master";
}

const targetsProtectedBranch = (positionals) =>
  positionals.flatMap((positional) => withoutPlus(positional).split(":")).some(isProtectedRef);

function isImplicitPush(refspecs) {
  return refspecs.length === 0 || refspecs.some((refspec) => CURRENT_BRANCH_REFS.has(withoutPlus(refspec)));
}

const isProtectedBranch = (branch) => branch === null || branch === "" || /^(main|master)$/i.test(branch);

function checkPushArguments(args, currentBranch) {
  const { forced, wholeRepository, positionals } = parsePushArguments(args);
  if (forced || positionals.some((positional) => positional.startsWith("+"))) return FORCED;
  if (wholeRepository || targetsProtectedBranch(positionals)) return PROTECTED;
  const [, ...refspecs] = positionals;
  if (isImplicitPush(refspecs) && isProtectedBranch(currentBranch())) return PROTECTED;
  return ALLOW;
}

// Comillas y barras invertidas dentro de una palabra pueden partir un nombre (`p"u"sh` en un alias de `-c`).
const plainWord = (word) => word.replaceAll(/["'\\]/g, "");
const mentionsPush = (words) => words.some((word) => /push/i.test(plainWord(word)));
const mentionsGit = (words) => words.some((word) => /\bgit\b/i.test(word));

// `hidden`: los argumentos del push pueden llegar por stdin o desde un fichero (xargs, parallel).
// Con configuración en línea, un built-in puede lanzar procesos hijo (`rebase -x`, `submodule foreach`) que ejecuten un alias.
// La configuración en línea (`-c`) solo importa si la orden menciona `push`; la del entorno no se puede inspeccionar.
function isOpaqueInvocation({ configured, unknown, subcommand, rest }, args, { hidden, envConfigured }) {
  if (hidden && (subcommand === "push" || mentionsPush(args))) return true;
  if (unknown && mentionsPush(args)) return true;
  if (!envConfigured && !(configured && mentionsPush(args))) return false;
  return !BUILTIN_SUBCOMMANDS.has(subcommand) || mentionsGit(rest);
}

function checkGitInvocation(args, { currentBranch, hidden, envConfigured }) {
  const options = readGlobalOptions(args);
  if (isOpaqueInvocation(options, args, { hidden, envConfigured })) return OPAQUE;
  return options.subcommand === "push" ? checkPushArguments(options.rest, currentBranch) : ALLOW;
}

function isNestedPush(word) {
  const plain = plainWord(word);
  return /git\b/i.test(plain) && /\bpush\b/i.test(plain);
}
const isGitInvoker = (word) => GIT_WORD.test(word) || GIT_PUSH_WORD.test(word);

function hasGitInvocation(command) {
  const commands = parseCommandList(stripDataHeredocs(command));
  return commands !== null && commands.some(({ words }) => words.some(isGitInvoker));
}

// Una palabra como la de `bash -c '…'` puede contener otra orden: se analiza aparte, con profundidad acotada.
// Si no contiene ninguna invocación de git (`env -S'git push …'`, `VAR="git push …"`), se aplica la regla anterior.
function checkNestedWord(word, { currentBranch, depth, hidden }) {
  if (depth >= MAX_NESTING) return legacyPushVerdict(word);
  const verdict = checkPush(word, { tool: "Bash", currentBranch, depth: depth + 1, hidden });
  if (!verdict.allow || hasGitInvocation(word)) return verdict;
  return legacyPushVerdict(word);
}

function checkWord(words, i, context) {
  const word = words[i];
  const hidden = context.hidden || words.slice(0, i).some((before) => BATCH_RUNNER.test(before));
  if (GIT_WORD.test(word)) return checkGitInvocation(words.slice(i + 1), { ...context, hidden });
  if (GIT_PUSH_WORD.test(word)) return checkGitInvocation(["push", ...words.slice(i + 1)], { ...context, hidden });
  return isNestedPush(word) ? checkNestedWord(word, { ...context, hidden }) : ALLOW;
}

const ALIAS_WORD = /^(?:un)?alias$/;

// Un alias puede esconder un push y usarse luego con cualquier rama: toda la orden pasa a ser opaca.
const definesGitAlias = ({ words }) => ALIAS_WORD.test(words[0] ?? "") && mentionsGit(words);

// El destino de una redirección (`<<< 'git push …'`) es una palabra anidada más.
function checkRedirects(redirects, context) {
  for (const { target } of redirects) {
    const verdict = isNestedPush(target) ? checkNestedWord(target, context) : ALLOW;
    if (!verdict.allow) return verdict;
  }
  return ALLOW;
}

// Una palabra reservada (`if`, `!`, `time`…) puede preceder a `git`: se busca en cualquier posición.
function checkSimpleCommand({ words, redirects }, context) {
  for (let i = 0; i < words.length; i += 1) {
    const verdict = checkWord(words, i, context);
    if (!verdict.allow) return verdict;
  }
  return checkRedirects(redirects, context);
}

/**
 * Evalúa una orden de shell contra R5.
 * `currentBranch` solo se consulta con un push sin destino explícito.
 */
export function checkPush(command, { tool, currentBranch, depth = 0, hidden = false }) {
  if (tool !== "Bash") return legacyPushVerdict(command);
  const commands = parseCommandList(stripDataHeredocs(command));
  if (commands === null) return legacyPushVerdict(command);
  const context = { currentBranch, envConfigured: ENV_CONFIG.test(command), depth, hidden };
  if (commands.some(definesGitAlias)) return OPAQUE;
  for (const command of commands) {
    const verdict = checkSimpleCommand(command, context);
    if (!verdict.allow) return verdict;
  }
  return ALLOW;
}
