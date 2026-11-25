// Memoria del equipo de agentes (R11). Única vía de escritura de docs/memory/.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { PreconditionError, UsageError } from "./lib/memory-errors.mjs";
import { formatDate, parseSessionInput, renderSession, sessionFileName } from "./lib/memory-entry.mjs";
import { check, tags } from "./lib/memory-check.mjs";
import { close } from "./lib/memory-close.mjs";
import { flush } from "./lib/memory-flush.mjs";
import { promote } from "./lib/memory-long.mjs";
import { findSessionByAgentId, nextN } from "./lib/memory-store.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BRANCH_RE = /^(?:feat|fix|chore)\/(\d{3}-[a-z0-9]+(?:-[a-z0-9]+)*)$/;
const AGENT_ID_RE = /^[A-Za-z0-9._-]{1,64}$/;
const TASK_RE = /^T\d+$/;
const AGENTS = ["developer", "reviewer"];
const VERDICTS = ["APPROVED", "CHANGES_REQUESTED"];
const CHECK_FAILED = 4;
const MEMORY_PREFIX = "docs/memory/";
const GATE_HEADER = "──── Quality gate ────";

const USAGE = [
  "Uso: node scripts/memory.mjs <subcomando>",
  "  session --agent <developer|reviewer> --agent-id <id> --task <Tn> [--veredicto <APPROVED|CHANGES_REQUESTED>]  (stdin)",
  "  flush --task <Tn> --agent-id <id>",
  "  close",
  "  promote",
  "  check",
  "  tags",
  "  help | --help",
].join("\n");

function parseOptions(args, allowed) {
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]?.startsWith("--") ? args[i].slice(2) : null;
    if (!allowed.includes(key)) throw new UsageError(`argumento no admitido '${args[i]}'\n${USAGE}`);
    if (args[i + 1] === undefined || key in options) throw new UsageError(`valor ausente o repetido para '--${key}'`);
    options[key] = args[i + 1];
  }
  return options;
}

function requireOption(options, key) {
  if (options[key] === undefined) throw new UsageError(`falta '--${key}'`);
  return options[key];
}

function requireTaskId(options) {
  const task = requireOption(options, "task");
  if (!TASK_RE.test(task)) throw new UsageError("'--task' debe cumplir ^T\\d+$");
  return task;
}

function requireAgentId(options) {
  const agentId = requireOption(options, "agent-id");
  if (!AGENT_ID_RE.test(agentId)) throw new UsageError("'--agent-id' debe cumplir ^[A-Za-z0-9._-]{1,64}$");
  return agentId;
}

function parseSessionOptions(args) {
  const options = parseOptions(args, ["agent", "agent-id", "task", "veredicto"]);
  const agent = requireOption(options, "agent");
  if (!AGENTS.includes(agent)) throw new UsageError(`'--agent' debe ser ${AGENTS.join(" o ")}`);
  const agentId = requireAgentId(options);
  const task = requireTaskId(options);
  return { agent, agentId, task, verdict: parseVerdict(agent, options.veredicto) };
}

function parseFlushOptions(args) {
  const options = parseOptions(args, ["task", "agent-id"]);
  return { task: requireTaskId(options), agentId: requireAgentId(options) };
}

function parseVerdict(agent, verdict) {
  if (agent === "developer" && verdict !== undefined) throw new UsageError("'--veredicto' está prohibido con developer");
  if (agent === "reviewer" && !VERDICTS.includes(verdict)) {
    throw new UsageError(`'--veredicto' es obligatorio con reviewer: ${VERDICTS.join(" o ")}`);
  }
  return verdict;
}

function resolvePlan({ root, git }) {
  const branch = (git(["branch", "--show-current"]) ?? "").trim();
  const slug = BRANCH_RE.exec(branch)?.[1];
  if (!slug) throw new PreconditionError(`la rama '${branch}' no corresponde a un plan (R10)`);
  const planRel = `docs/plans/${slug}.md`;
  if (!existsSync(join(root, "docs", "plans", `${slug}.md`))) {
    throw new PreconditionError(`no existe el plan ${planRel}`);
  }
  return { branch, slug, planRel };
}

function readOrFail(path, label) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    throw new PreconditionError(`no se puede leer ${label}: ${error.message}`);
  }
}

function requireTask(root, planRel, task) {
  const text = readOrFail(join(root, ...planRel.split("/")), planRel);
  if (!new RegExp(`^### ${task}\\s`, "m").test(text)) {
    throw new PreconditionError(`no existe la tarea ${task} en ${planRel}`);
  }
}

function requireFreshAgentId(root, agentId) {
  const existing = findSessionByAgentId(root, agentId);
  if (existing) throw new PreconditionError(`el agent_id '${agentId}' ya tiene sesión: ${existing.rel}`);
}

// Con -z, los renombrados y copias añaden la ruta de origen como entrada aparte.
function changedFiles(statusOutput) {
  const entries = statusOutput.split("\0").filter(Boolean);
  const files = [];
  for (let i = 0; i < entries.length; i += 1) {
    files.push(entries[i].slice(3));
    if (/^[RC]/.test(entries[i])) i += 1;
  }
  return files.filter((file) => !file.startsWith(MEMORY_PREFIX)).sort();
}

function readChangedFiles(git) {
  const args = ["-c", "core.quotePath=false", "status", "--porcelain=v1", "-z", "--untracked-files=all"];
  const output = git(args);
  if (output === null) throw new PreconditionError("git status ha fallado: no se pueden calcular los ficheros cambiados");
  return changedFiles(output);
}

function sessionMeta({ context, options, date, n, files, gate }) {
  const meta = {
    fecha: date,
    plan: context.slug,
    tarea: options.task,
    agente: options.agent,
    agentId: options.agentId,
    n,
    rama: context.branch,
    gate: gate.ok ? "verde" : "rojo",
    files,
  };
  if (!gate.ok) meta.gateFailed = gate.failed;
  if (options.verdict) meta.veredicto = options.verdict;
  return meta;
}

function writeSession(root, name, content) {
  const dir = join(root, "docs", "memory", "short");
  try {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, name), content, { encoding: "utf8", flag: "wx" });
  } catch (error) {
    throw new PreconditionError(`no se puede escribir ${MEMORY_PREFIX}short/${name}: ${error.message}`);
  }
}

async function runSession(args, deps) {
  const options = parseSessionOptions(args);
  const context = resolvePlan(deps);
  requireTask(deps.root, context.planRel, options.task);
  const input = parseSessionInput(deps.readStdin());
  requireFreshAgentId(deps.root, options.agentId);
  const files = readChangedFiles(deps.git);
  const gate = deps.runGate();
  const date = formatDate(deps.now());
  const n = nextN(deps.root, `${date}-${context.slug}-${options.task}-${options.agent}-`);
  const meta = sessionMeta({ context, options, date, n, files, gate });
  const name = sessionFileName(meta);
  writeSession(deps.root, name, renderSession(meta, input));
  deps.out(`docs/memory/short/${name}`);
}

function runFlush(args, deps) {
  const options = parseFlushOptions(args);
  const context = resolvePlan(deps);
  const { rel, cycle, deleted } = flush(deps.root, context.slug, options.task, options.agentId);
  deps.out(rel);
  deps.out(`ciclo: ${options.task} C${cycle}`);
  deps.out(`borradas: ${deleted}`);
}

function reportPromotion({ written, dropped }, deps) {
  written.forEach((rel) => deps.out(rel));
  dropped.forEach(({ kind, slug }) => deps.out(`descartado por límite: ${kind} ${slug}`));
}

function runClose(args, deps) {
  parseOptions(args, []);
  const context = resolvePlan(deps);
  const { rel, long } = close(deps.root, context.slug);
  deps.out(rel);
  reportPromotion(long, deps);
}

function runPromote(args, deps) {
  parseOptions(args, []);
  reportPromotion(promote(deps.root), deps);
}

function runHelp(deps) {
  deps.out(USAGE);
}

// Los subcomandos devuelven un código de salida o nada: nunca el resultado de `out`.
function runCheck(args, deps) {
  parseOptions(args, []);
  const { issues, plans, closed } = check(deps.root);
  if (issues.length === 0) {
    deps.out(`Memoria OK (${plans} planes, ${closed} cerrados)`);
    return undefined;
  }
  issues.forEach((issue) => deps.err(`[R11] ${issue}`));
  return CHECK_FAILED;
}

function runTags(args, deps) {
  parseOptions(args, []);
  tags(deps.root).forEach((line) => deps.out(line));
}

function dispatch(argv, deps) {
  const [command, ...args] = argv;
  if (command === undefined) throw new UsageError(`falta el subcomando\n${USAGE}`);
  if (command === "help" || command === "--help") return runHelp(deps);
  if (command === "session") return runSession(args, deps);
  if (command === "flush") return runFlush(args, deps);
  if (command === "close") return runClose(args, deps);
  if (command === "promote") return runPromote(args, deps);
  if (command === "check") return runCheck(args, deps);
  if (command === "tags") return runTags(args, deps);
  throw new UsageError(`subcomando desconocido '${command}'\n${USAGE}`);
}

export async function run(argv, deps) {
  try {
    return (await dispatch(argv, deps)) ?? 0;
  } catch (error) {
    if (![1, 2, 3].includes(error.code)) throw error;
    deps.err(`[R11] ${error.message}`);
    return error.code;
  }
}

export function failedGateSteps(output) {
  const afterHeader = output.split(GATE_HEADER)[1] ?? "";
  return afterHeader
    .split(/\r?\n/)
    .filter((line) => line.startsWith("✖ "))
    .map((line) => line.slice(2).trim());
}

function runQualityGate(root) {
  const result = spawnSync(process.execPath, [join(root, "scripts", "quality-gate.mjs")], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return { ok: result.status === 0, failed: failedGateSteps(result.stdout ?? "") };
}

function runGit(root, args) {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  return result.status === 0 ? result.stdout : null;
}

function realDeps(root) {
  return {
    root,
    now: () => new Date(),
    git: (args) => runGit(root, args),
    runGate: () => runQualityGate(root),
    readStdin: () => readFileSync(0, "utf8"),
    out: (text) => process.stdout.write(`${text}\n`),
    err: (text) => process.stderr.write(`${text}\n`),
  };
}

// Node resuelve los enlaces simbólicos en import.meta.url, así que argv[1] se resuelve igual.
// Con `node -e`, argv[1] puede no ser una ruta real: entonces no somos el script principal.
function isMainModule() {
  try {
    return realpathSync(process.argv[1]) === fileURLToPath(import.meta.url);
  } catch {
    return false;
  }
}

if (isMainModule()) {
  process.exitCode = await run(process.argv.slice(2), realDeps(ROOT));
}
