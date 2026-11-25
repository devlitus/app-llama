// Utilidades compartidas por los hooks de Claude Code.
// Contrato: exit 0 = permitir; exit 2 + stderr = bloquear y explicar el motivo a Claude.

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

export const PROJECT_DIR = process.env.CLAUDE_PROJECT_DIR ?? process.cwd();

export function readHookInput() {
  try {
    return JSON.parse(readFileSync(0, "utf8"));
  } catch {
    return {};
  }
}

export function block(reason) {
  process.stderr.write(`BLOQUEADO por governance: ${reason}\nVer docs/GOVERNANCE.md\n`);
  process.exit(2);
}

export function git(args) {
  const res = spawnSync("git", args, { cwd: PROJECT_DIR, encoding: "utf8" });
  return res.status === 0 ? res.stdout.trim() : null;
}

export const PROTECTED_PATHS = [
  /^\.claude\/settings\.json$/,
  /^\.claude\/hooks\//,
  /^\.githooks\//,
  /^scripts\/quality-gate\.mjs$/,
  /^scripts\/memory(\.test)?\.mjs$/,
  /^scripts\/lib\//,
  /^\.github\/workflows\//,
  /^docs\/GOVERNANCE\.md$/,
];

export function toProjectRelative(filePath) {
  const norm = (p) => p.replace(/\\/g, "/").toLowerCase();
  const root = norm(PROJECT_DIR).replace(/\/$/, "") + "/";
  const file = norm(filePath);
  return file.startsWith(root) ? file.slice(root.length) : file;
}

export function isProtected(filePath) {
  const rel = toProjectRelative(filePath);
  return PROTECTED_PATHS.some((re) => re.test(rel));
}
