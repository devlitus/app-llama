import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { PreconditionError } from "./memory-errors.mjs";
import { parseSession } from "./memory-entry.mjs";

const SHORT_REL = "docs/memory/short";

function shortDir(root) {
  return join(root, "docs", "memory", "short");
}

function shortFileNames(root) {
  const dir = shortDir(root);
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => name.endsWith(".md"))
    .sort();
}

function readShortFile(root, name) {
  try {
    return readFileSync(join(shortDir(root), name), "utf8");
  } catch (error) {
    throw new PreconditionError(`sesión ilegible ${SHORT_REL}/${name}: ${error.message}`);
  }
}

function readSession(root, name) {
  const rel = `${SHORT_REL}/${name}`;
  const text = readShortFile(root, name);
  try {
    return { rel, ...parseSession(text) };
  } catch (error) {
    throw new PreconditionError(`sesión ilegible ${rel}: ${error.message}`);
  }
}

export function listSessions(root) {
  return shortFileNames(root).map((name) => readSession(root, name));
}

export function nextN(root, prefix) {
  const numbers = shortFileNames(root)
    .filter((name) => name.startsWith(prefix))
    .map((name) => /^(\d+)\.md$/.exec(name.slice(prefix.length)))
    .filter(Boolean)
    .map((match) => Number(match[1]));
  return Math.max(0, ...numbers) + 1;
}

function tryParseMeta(text) {
  try {
    return parseSession(text).meta;
  } catch {
    return null;
  }
}

export function findSessionByAgentId(root, agentId) {
  const wanted = `agent_id: ${agentId}`;
  for (const name of shortFileNames(root)) {
    const text = readShortFile(root, name);
    if (text.split(/\r?\n/).includes(wanted)) {
      return { rel: `${SHORT_REL}/${name}`, meta: tryParseMeta(text) };
    }
  }
  return null;
}

export function sessionsOfTask(root, plan, task) {
  return listSessions(root).filter(({ meta }) => meta.plan === plan && meta.tarea === task);
}
