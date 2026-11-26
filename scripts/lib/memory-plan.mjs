import { compareBySlug, compareTasks, compareText } from "./memory-order.mjs";

const SCHEMA = "memoria-plan/2";
const ESTADOS = ["abierto", "cerrado"];
const PATTERNS_HEADING = "Patrones del plan";
const CYCLE_HEADER = [
  "| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |",
  "|---|---|---|---|---|---|",
];

export function lastCycle({ cycles }) {
  return cycles.reduce((latest, current) => (latest === undefined || current.cycle > latest.cycle ? current : latest), undefined);
}

function renderFrontMatter(doc) {
  return ["---", `schema: ${SCHEMA}`, `plan: ${doc.plan}`, `estado: ${doc.estado}`, "---"].join("\n");
}

function renderList(lines, empty) {
  return lines.length > 0 ? lines.join("\n") : empty;
}

function renderCycleRow(cycle) {
  const cells = [cycle.cycle, cycle.fecha, cycle.developer, cycle.reviewer, cycle.gate, cycle.veredicto];
  return `| ${cells.join(" | ")} |`;
}

const cycleLabels = ({ cycles }) => cycles.map((cycle) => `C${cycle}`);
const taskLabels = ({ tasks }) => tasks;

function renderSuffix(labels, issues) {
  const tail = issues?.length > 0 ? ` · ${issues.map((issue) => `#${issue}`).join(", ")}` : "";
  return `(${labels.join(", ")}${tail})`;
}

function renderEntry(entry, labelsOf) {
  return `- **${entry.slug}** — ${entry.text} ${renderSuffix(labelsOf(entry), entry.issues)}`;
}

function renderDecisions(entries, labelsOf) {
  return renderList(entries.map((entry) => renderEntry(entry, labelsOf)), "- (ninguna)");
}

function renderErrors(entries, labelsOf) {
  return renderList(entries.map((entry) => renderEntry(entry, labelsOf)), "- (ninguno)");
}

function renderCycles(task) {
  return ["### Ciclos", "", ...CYCLE_HEADER, ...task.cycles.map(renderCycleRow)];
}

function renderOpenTask(task) {
  const summaries = task.summaries.map(({ cycle, role, text }) => `- C${cycle} · ${role} · ${text}`);
  return [
    `## ${task.task}`,
    "",
    ...renderCycles(task),
    "",
    "### Resúmenes",
    "",
    renderList(summaries, "- (ninguno)"),
    "",
    "### Decisiones",
    "",
    renderDecisions(task.decisions, cycleLabels),
    "",
    "### Errores",
    "",
    renderErrors(task.errors, cycleLabels),
  ].join("\n");
}

const renderClosedTask = (task) => [`## ${task.task}`, "", ...renderCycles(task)].join("\n");

function renderPatterns({ decisions, errors }) {
  return [
    `## ${PATTERNS_HEADING}`,
    "",
    "### Decisiones",
    "",
    renderDecisions(decisions, taskLabels),
    "",
    "### Errores",
    "",
    renderErrors(errors, taskLabels),
  ].join("\n");
}

function renderSections(doc) {
  if (doc.estado === "abierto") return doc.tasks.map(renderOpenTask);
  return [renderPatterns(doc.patterns), ...doc.tasks.map(renderClosedTask)];
}

export function renderPlan(doc) {
  const blocks = [renderFrontMatter(doc), `# Memoria del plan ${doc.plan}`, ...renderSections(doc)];
  return `${blocks.join("\n\n")}\n`;
}

function parseFrontMatter(md) {
  const match = /^---\n([\s\S]*?)\n---\n([\s\S]*)$/.exec(md);
  if (!match) throw new Error("medio plazo sin front matter");
  const fields = new Map();
  for (const line of match[1].split("\n")) {
    const separator = line.indexOf(":");
    if (separator > 0) fields.set(line.slice(0, separator), line.slice(separator + 1).trim());
  }
  if (fields.get("schema") !== SCHEMA) throw new Error(`schema desconocido '${fields.get("schema")}'`);
  return { fields, body: match[2] };
}

function bullets(section = "") {
  return section.split("\n").filter((line) => line.startsWith("- ") && !/^- \(ning/.test(line));
}

const CYCLE_ROW_RE = /^\| (\d+) \| (\d{4}-\d{2}-\d{2}) \| (\d+) \| ([A-Za-z0-9._-]+) \| (verde|rojo) \| (APPROVED|CHANGES_REQUESTED) \|$/;
const SUMMARY_RE = /^- C(\d+) · (developer|reviewer) · (.+)$/;
// El sufijo ancla al final de la línea: un texto con paréntesis se recupera literal.
const ISSUES = "(?: · (#\\d+(?:, #\\d+)*))?";
const CYCLE_ENTRY_RE = new RegExp(`^- \\*\\*(.+?)\\*\\* — (.*) \\(C(\\d+(?:, C\\d+)*)${ISSUES}\\)$`);
const TASK_ENTRY_RE = new RegExp(`^- \\*\\*(.+?)\\*\\* — (.*) \\((T\\d+(?:, T\\d+)*)${ISSUES}\\)$`);

function matchOrFail(re, line, what) {
  const match = re.exec(line);
  if (!match) throw new Error(`${what} mal formado: '${line}'`);
  return match;
}

function parseCycleRow(line) {
  const [, cycle, fecha, developer, reviewer, gate, veredicto] = matchOrFail(CYCLE_ROW_RE, line, "fila de ciclos");
  return { cycle: Number(cycle), fecha, developer: Number(developer), reviewer, gate, veredicto };
}

function parseCycles(section = "") {
  const rows = section.split("\n").filter((line) => line.startsWith("| ")).slice(1);
  return rows.map(parseCycleRow);
}

function parseSummary(line) {
  const [, cycle, role, text] = matchOrFail(SUMMARY_RE, line, "resumen");
  return { cycle: Number(cycle), role, text };
}

function parseIssues(issues) {
  return issues ? issues.split(", ").map((issue) => Number(issue.slice(1))) : [];
}

function parseCycleEntry(line) {
  const [, slug, text, cycles, issues] = matchOrFail(CYCLE_ENTRY_RE, line, "entrada");
  return { entry: { slug, text, cycles: cycles.split(", C").map(Number) }, issues: parseIssues(issues) };
}

function parseTaskEntry(line) {
  const [, slug, text, tasks, issues] = matchOrFail(TASK_ENTRY_RE, line, "entrada");
  return { entry: { slug, text, tasks: tasks.split(", ") }, issues: parseIssues(issues) };
}

const decisionParser = (parseEntry) => (line) => parseEntry(line).entry;
const errorParser = (parseEntry) => (line) => {
  const { entry, issues } = parseEntry(line);
  return { ...entry, issues };
};

function splitByHeading(text, level) {
  const sections = new Map();
  for (const chunk of text.split(new RegExp(`^${level} `, "m")).slice(1)) {
    const [heading, ...lines] = chunk.split("\n");
    sections.set(heading.trim(), lines.join("\n").trim());
  }
  return sections;
}

function requireTaskName(task) {
  if (!/^T\d+$/.test(task)) throw new Error(`tarea inválida '${task}'`);
}

// Orden estricto: un slug o una tarea repetidos o fuera de orden harían que el recuento de planes mintiera.
function requireStrictOrder(values, compare, what) {
  for (let i = 1; i < values.length; i += 1) {
    if (compare(values[i - 1], values[i]) >= 0) throw new Error(`${what} repetido o desordenado: '${values[i]}'`);
  }
}

function requireSlugOrder(entries) {
  requireStrictOrder(entries.map(({ slug }) => slug), compareText, "slug");
  return entries;
}

function requireTaskOrder(tasks) {
  requireStrictOrder(tasks.map(({ task }) => task), compareTasks, "tarea");
  return tasks;
}

function parseOpenTask(task, body) {
  requireTaskName(task);
  const sections = splitByHeading(body, "###");
  return {
    task,
    cycles: parseCycles(sections.get("Ciclos")),
    summaries: bullets(sections.get("Resúmenes")).map(parseSummary),
    decisions: requireSlugOrder(bullets(sections.get("Decisiones")).map(decisionParser(parseCycleEntry))),
    errors: requireSlugOrder(bullets(sections.get("Errores")).map(errorParser(parseCycleEntry))),
  };
}

function parseClosedTask(task, body) {
  requireTaskName(task);
  return { task, cycles: parseCycles(splitByHeading(body, "###").get("Ciclos")) };
}

function parsePatterns(body) {
  if (body === undefined) throw new Error(`falta la sección '${PATTERNS_HEADING}'`);
  const sections = splitByHeading(body, "###");
  return {
    decisions: requireSlugOrder(bullets(sections.get("Decisiones")).map(decisionParser(parseTaskEntry))),
    errors: requireSlugOrder(bullets(sections.get("Errores")).map(errorParser(parseTaskEntry))),
  };
}

function parseClosed(doc, sections) {
  const patterns = parsePatterns(sections.get(PATTERNS_HEADING));
  sections.delete(PATTERNS_HEADING);
  return { ...doc, patterns, tasks: requireTaskOrder([...sections].map(([task, body]) => parseClosedTask(task, body))) };
}

function parseOpen(doc, sections) {
  return { ...doc, tasks: requireTaskOrder([...sections].map(([task, body]) => parseOpenTask(task, body))) };
}

// Un medio plazo debe ser canónico (render(parse(x)) == x): así nada se pierde al reescribirlo.
export function parsePlan(md) {
  const text = md.replace(/\r\n/g, "\n");
  const { fields, body } = parseFrontMatter(text);
  const estado = fields.get("estado");
  if (!ESTADOS.includes(estado)) throw new Error(`estado desconocido '${estado}'`);
  const doc = { plan: fields.get("plan"), estado };
  const sections = splitByHeading(body, "##");
  const parsed = estado === "abierto" ? parseOpen(doc, sections) : parseClosed(doc, sections);
  if (renderPlan(parsed) !== text) throw new Error("formato no canónico");
  return parsed;
}

const ascending = (a, b) => a - b;

// Las fechas ISO se ordenan como texto; `n` se reinicia cada día, así que va después de la fecha y como número.
function compareSessions(a, b) {
  if (a.meta.fecha !== b.meta.fecha) return a.meta.fecha < b.meta.fecha ? -1 : 1;
  return a.meta.n - b.meta.n;
}

function upsertEntry(entries, { slug, text, issues }, cycle) {
  const index = entries.findIndex((entry) => entry.slug === slug);
  const previous = index >= 0 ? entries[index] : { slug, cycles: [], issues: [] };
  const cycles = previous.cycles.includes(cycle) ? previous.cycles : [...previous.cycles, cycle];
  const merged = { slug, text, cycles };
  if (issues) merged.issues = [...new Set([...previous.issues, ...issues])].sort(ascending);
  const rest = entries.filter((entry) => entry.slug !== slug);
  return [...rest, merged].sort(compareBySlug);
}

function mergeEntries(entries, incoming, cycle) {
  return incoming.reduce((acc, entry) => upsertEntry(acc, entry, cycle), entries);
}

function emptyTask(task) {
  return { task, cycles: [], summaries: [], decisions: [], errors: [] };
}

function buildCycle(cycle, devCount, { meta }) {
  return {
    cycle,
    fecha: meta.fecha,
    developer: devCount,
    reviewer: meta.agentId,
    gate: meta.gate,
    veredicto: meta.veredicto,
  };
}

function mergeTask(task, devSessions, reviewerSession) {
  const cycle = task.cycles.length + 1;
  const sessions = [...devSessions, reviewerSession];
  return {
    ...task,
    cycles: [...task.cycles, buildCycle(cycle, devSessions.length, reviewerSession)],
    summaries: [
      ...task.summaries,
      ...sessions.map(({ meta, input }) => ({ cycle, role: meta.agente, text: input.summary })),
    ],
    decisions: sessions.reduce((acc, { input }) => mergeEntries(acc, input.decisions, cycle), task.decisions),
    errors: sessions.reduce((acc, { input }) => mergeEntries(acc, input.errors, cycle), task.errors),
  };
}

export function mergeCycle(doc, task, devSessions, reviewerSession) {
  const current = doc.tasks.find((entry) => entry.task === task) ?? emptyTask(task);
  const merged = mergeTask(current, [...devSessions].sort(compareSessions), reviewerSession);
  const tasks = [...doc.tasks.filter((entry) => entry.task !== task), merged];
  return { ...doc, tasks: tasks.sort((a, b) => compareTasks(a.task, b.task)) };
}

function mergePattern(patterns, entry, task) {
  const previous = patterns.get(entry.slug);
  const merged = { slug: entry.slug, text: entry.text, tasks: [...(previous?.tasks ?? []), task] };
  if (entry.issues) merged.issues = [...new Set([...(previous?.issues ?? []), ...entry.issues])].sort(ascending);
  patterns.set(entry.slug, merged);
}

function collectPatterns(tasks, field) {
  const patterns = new Map();
  for (const task of tasks) task[field].forEach((entry) => mergePattern(patterns, entry, task.task));
  return [...patterns.values()].sort(compareBySlug);
}

// El texto de cada patrón es el de la última tarea (en orden numérico) que lo menciona.
export function compactPlan(doc) {
  const tasks = [...doc.tasks].sort((a, b) => compareTasks(a.task, b.task));
  return {
    plan: doc.plan,
    estado: "cerrado",
    patterns: { decisions: collectPatterns(tasks, "decisions"), errors: collectPatterns(tasks, "errors") },
    tasks: tasks.map(({ task, cycles }) => ({ task, cycles })),
  };
}
