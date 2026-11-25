const SCHEMA = "memoria-plan/2";
const ESTADOS = ["abierto", "cerrado"];
const CYCLE_HEADER = [
  "| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |",
  "|---|---|---|---|---|---|",
];

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

function renderSuffix({ cycles, issues }) {
  const labels = cycles.map((cycle) => `C${cycle}`).join(", ");
  const tail = issues?.length > 0 ? ` · ${issues.map((issue) => `#${issue}`).join(", ")}` : "";
  return `(${labels}${tail})`;
}

function renderEntry(entry) {
  return `- **${entry.slug}** — ${entry.text} ${renderSuffix(entry)}`;
}

function renderTask(task) {
  const summaries = task.summaries.map(({ cycle, role, text }) => `- C${cycle} · ${role} · ${text}`);
  return [
    `## ${task.task}`,
    "",
    "### Ciclos",
    "",
    ...CYCLE_HEADER,
    ...task.cycles.map(renderCycleRow),
    "",
    "### Resúmenes",
    "",
    renderList(summaries, "- (ninguno)"),
    "",
    "### Decisiones",
    "",
    renderList(task.decisions.map(renderEntry), "- (ninguna)"),
    "",
    "### Errores",
    "",
    renderList(task.errors.map(renderEntry), "- (ninguno)"),
  ].join("\n");
}

export function renderPlan(doc) {
  const blocks = [renderFrontMatter(doc), `# Memoria del plan ${doc.plan}`, ...doc.tasks.map(renderTask)];
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
const ENTRY_RE = /^- \*\*(.+?)\*\* — (.*) \(C(\d+(?:, C\d+)*)(?: · (#\d+(?:, #\d+)*))?\)$/;

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

function parseEntry(line) {
  const [, slug, text, cycles, issues] = matchOrFail(ENTRY_RE, line, "entrada");
  const entry = { slug, text, cycles: cycles.split(", C").map(Number) };
  return { entry, issues: issues ? issues.split(", ").map((issue) => Number(issue.slice(1))) : [] };
}

const parseDecision = (line) => parseEntry(line).entry;
const parseError = (line) => {
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

function parseTask(task, body) {
  if (!/^T\d+$/.test(task)) throw new Error(`tarea inválida '${task}'`);
  const sections = splitByHeading(body, "###");
  return {
    task,
    cycles: parseCycles(sections.get("Ciclos")),
    summaries: bullets(sections.get("Resúmenes")).map(parseSummary),
    decisions: bullets(sections.get("Decisiones")).map(parseDecision),
    errors: bullets(sections.get("Errores")).map(parseError),
  };
}

// Solo el formato abierto tiene detalle por tarea; un plan cerrado se reconoce por su cabecera.
// Un medio plazo abierto debe ser canónico (render(parse(x)) == x): así nada se pierde al reescribirlo.
export function parsePlan(md) {
  const text = md.replace(/\r\n/g, "\n");
  const { fields, body } = parseFrontMatter(text);
  const estado = fields.get("estado");
  if (!ESTADOS.includes(estado)) throw new Error(`estado desconocido '${estado}'`);
  const doc = { plan: fields.get("plan"), estado, tasks: [] };
  if (estado === "cerrado") return doc;
  doc.tasks = [...splitByHeading(body, "##")].map(([task, taskBody]) => parseTask(task, taskBody));
  if (renderPlan(doc) !== text) throw new Error("no está en formato canónico");
  return doc;
}

const taskNumber = (task) => Number(task.slice(1));
const bySlug = (a, b) => (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0);

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
  if (issues) merged.issues = [...new Set([...previous.issues, ...issues])].sort((a, b) => a - b);
  const rest = entries.filter((entry) => entry.slug !== slug);
  return [...rest, merged].sort(bySlug);
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
  return { ...doc, tasks: tasks.sort((a, b) => taskNumber(a.task) - taskNumber(b.task)) };
}
