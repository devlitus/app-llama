import { findSecrets } from "./secrets.mjs";

const SLUG_RE = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const SLUG_MAX = 40;
const TEXT_MAX = 200;
const SUMMARY_MAX = 300;
const ENTRIES_MAX = 10;
const SCHEMA = "memoria-sesion/1";

export class MemoryInputError extends Error {
  constructor(message) {
    super(message);
    this.name = "MemoryInputError";
    this.code = 2;
  }
}

function fail(lineNo, message) {
  throw new MemoryInputError(`línea ${lineNo}: ${message}`);
}

export function extractIssues(text) {
  const numbers = (text.match(/#\d+/g) ?? []).map((tag) => Number(tag.slice(1)));
  return [...new Set(numbers)].sort((a, b) => a - b);
}

function checkText(value, max, lineNo) {
  if (value === "") fail(lineNo, "texto vacío");
  if (value.length > max) fail(lineNo, `texto demasiado largo (máximo ${max})`);
}

function splitSlug(rest, lineNo) {
  const match = /^\[([^\]]*)\](?:\s+(.*))?$/s.exec(rest);
  if (!match) fail(lineNo, "falta el slug entre corchetes: '[slug] texto'");
  const [, slug, text] = match;
  if (slug.length > SLUG_MAX || !SLUG_RE.test(slug)) {
    fail(lineNo, `slug inválido '${slug}' (minúsculas, dígitos y guiones, máximo ${SLUG_MAX})`);
  }
  return { slug, text: (text ?? "").trim() };
}

function addEntry(list, kind, rest, lineNo) {
  if (list.length >= ENTRIES_MAX) fail(lineNo, `demasiadas entradas '${kind}' (máximo ${ENTRIES_MAX})`);
  const entry = splitSlug(rest, lineNo);
  checkText(entry.text, TEXT_MAX, lineNo);
  if (list.some(({ slug }) => slug === entry.slug)) fail(lineNo, `slug repetido '${entry.slug}'`);
  list.push(kind === "error" ? { ...entry, issues: extractIssues(entry.text) } : entry);
}

function setSummary(state, value, lineNo) {
  if (state.summary !== null) fail(lineNo, "solo se admite un 'resumen'");
  checkText(value, SUMMARY_MAX, lineNo);
  // El resumen ocupa línea propia: un '#' inicial se leería como cabecera Markdown.
  if (value.startsWith("#")) fail(lineNo, "el resumen no puede empezar por '#'");
  state.summary = value;
}

function parseLine(line, lineNo, state) {
  const [secret] = findSecrets(line);
  if (secret) fail(lineNo, `posible secreto (${secret.kind})`);
  const pair = /^([^\s:]+):(?: (.*))?$/.exec(line);
  if (!pair) fail(lineNo, "se esperaba 'clave: valor'");
  const key = pair[1];
  const value = (pair[2] ?? "").trim();
  if (key === "resumen") setSummary(state, value, lineNo);
  else if (key === "decision") addEntry(state.decisions, "decision", value, lineNo);
  else if (key === "error") addEntry(state.errors, "error", value, lineNo);
  else fail(lineNo, `clave desconocida '${key}'`);
}

export function parseSessionInput(text) {
  const state = { summary: null, decisions: [], errors: [] };
  text.split("\n").forEach((raw, index) => {
    const line = raw.replace(/\r$/, "");
    if (line.trim() !== "") parseLine(line, index + 1, state);
  });
  if (state.summary === null) throw new MemoryInputError("falta 'resumen'");
  return state;
}

export function formatDate(date) {
  return date.toISOString().slice(0, 10);
}

export function sessionFileName({ fecha, plan, tarea, agente, n }) {
  return `${fecha}-${plan}-${tarea}-${agente}-${n}.md`;
}

function renderGate(meta) {
  const failed = meta.gateFailed ?? [];
  return meta.gate === "rojo" && failed.length > 0 ? `rojo (${failed.join(", ")})` : meta.gate;
}

function renderFrontMatter(meta) {
  const lines = [
    `schema: ${SCHEMA}`,
    `fecha: ${meta.fecha}`,
    `plan: ${meta.plan}`,
    `tarea: ${meta.tarea}`,
    `agente: ${meta.agente}`,
    `agent_id: ${meta.agentId}`,
    `n: ${meta.n}`,
    `rama: ${meta.rama}`,
    `gate: ${renderGate(meta)}`,
  ];
  if (meta.veredicto) lines.push(`veredicto: ${meta.veredicto}`);
  return ["---", ...lines, "---"].join("\n");
}

function renderList(items, empty) {
  return items.length > 0 ? items.join("\n") : empty;
}

export function renderSession(meta, input) {
  const files = meta.files.map((file) => `- \`${file}\``);
  const decisions = input.decisions.map(({ slug, text }) => `- **${slug}** — ${text}`);
  const errors = input.errors.map(({ slug, text }) => `- **${slug}** — ${text}`);
  return [
    renderFrontMatter(meta),
    "",
    `# Sesión ${meta.plan} · ${meta.tarea} · ${meta.agente} · ${meta.n}`,
    "",
    "## Resumen",
    "",
    input.summary,
    "",
    "## Ficheros cambiados",
    "",
    renderList(files, "- (ninguno)"),
    "",
    "## Decisiones",
    "",
    renderList(decisions, "- (ninguna)"),
    "",
    "## Errores",
    "",
    renderList(errors, "- (ninguno)"),
    "",
  ].join("\n");
}

function splitFrontMatter(md) {
  const match = /^---\n([\s\S]*?)\n---\n([\s\S]*)$/.exec(md);
  if (!match) throw new Error("sesión sin front matter");
  const fields = new Map();
  for (const line of match[1].split("\n")) {
    const separator = line.indexOf(":");
    if (separator > 0) fields.set(line.slice(0, separator), line.slice(separator + 1).trim());
  }
  return { fields, body: match[2] };
}

function parseGate(value) {
  if (value === "rojo") return { gate: "rojo", gateFailed: [] };
  const match = /^rojo \((.*)\)$/.exec(value);
  return match ? { gate: "rojo", gateFailed: match[1].split(", ") } : { gate: value };
}

function parseMeta(fields) {
  if (fields.get("schema") !== SCHEMA) throw new Error(`schema desconocido '${fields.get("schema")}'`);
  const meta = {
    fecha: fields.get("fecha"),
    plan: fields.get("plan"),
    tarea: fields.get("tarea"),
    agente: fields.get("agente"),
    agentId: fields.get("agent_id"),
    n: Number(fields.get("n")),
    rama: fields.get("rama"),
    ...parseGate(fields.get("gate") ?? ""),
  };
  if (fields.has("veredicto")) meta.veredicto = fields.get("veredicto");
  return meta;
}

function splitSections(body) {
  const sections = new Map();
  for (const chunk of body.split(/^## /m).slice(1)) {
    const [heading, ...lines] = chunk.split("\n");
    sections.set(heading.trim(), lines.join("\n").trim());
  }
  return sections;
}

function bulletLines(section = "") {
  return section.split("\n").filter((line) => line.startsWith("- ") && !/^- \(ning/.test(line));
}

function parseSluggedEntries(section) {
  return bulletLines(section).map((line) => {
    const [, slug, text] = /^- \*\*(.+?)\*\* — (.*)$/.exec(line) ?? [];
    return { slug, text };
  });
}

export function parseSession(md) {
  const { fields, body } = splitFrontMatter(md.replace(/\r\n/g, "\n"));
  const sections = splitSections(body);
  const files = bulletLines(sections.get("Ficheros cambiados")).map((line) => line.slice(3, -1));
  const errors = parseSluggedEntries(sections.get("Errores")).map((entry) => ({
    ...entry,
    issues: extractIssues(entry.text),
  }));
  return {
    meta: { ...parseMeta(fields), files },
    input: {
      summary: sections.get("Resumen") ?? "",
      decisions: parseSluggedEntries(sections.get("Decisiones")),
      errors,
    },
  };
}
