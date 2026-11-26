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
