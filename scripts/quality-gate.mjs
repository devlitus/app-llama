#!/usr/bin/env node
// Quality gate único del proyecto. Lo usan los hooks de Claude Code, el git hook
// pre-commit y la CI. Sale con código 0 solo si todos los pasos pasan.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { findSecrets } from "./lib/secrets.mjs";
import { planSteps } from "./lib/gate-steps.mjs";

const ROOT = join(fileURLToPath(import.meta.url), "..", "..");

const SOURCE_DIRS = ["crates", "src-tauri/src", "ui/src"];
const SOURCE_EXTS = new Set([".rs", ".ts", ".tsx", ".js", ".mjs", ".svelte"]);
const SKIP_DIRS = new Set(["node_modules", "target", "dist", "build", ".svelte-kit"]);
const SECRET_DIRS = [...SOURCE_DIRS, "scripts", "docs/memory"];
const SECRET_EXTS = new Set([...SOURCE_EXTS, ".md", ".json", ".toml", ".yml", ".yaml"]);

const FORBIDDEN = [
  { re: /#!?\[\s*allow\s*\(/, why: "#[allow(...)] silencia el linter" },
  { re: /#\[\s*ignore\s*\]/, why: "#[ignore] desactiva un test" },
  { re: /\b(dbg|todo|unimplemented)!\s*\(/, why: "macro de depuración o pendiente" },
  { re: /eslint-disable/, why: "eslint-disable silencia el linter" },
  { re: /@ts-(ignore|nocheck)/, why: "silencia el type checker" },
  { re: /\b(it|test|describe)\.(only|skip)\s*\(/, why: ".only/.skip altera la suite de tests" },
];

function run(label, cmd) {
  process.stdout.write(`\n▶ ${label}\n  $ ${cmd}\n`);
  const res = spawnSync(cmd, { cwd: ROOT, stdio: "inherit", shell: true });
  return res.status === 0;
}

function walk(dir, out, exts = SOURCE_EXTS) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    if (SKIP_DIRS.has(name)) continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out, exts);
    else if (exts.has(extname(name))) out.push(full);
  }
  return out;
}

function scanForbiddenPatterns() {
  process.stdout.write("\n▶ Patrones prohibidos (governance R2)\n");
  const violations = [];
  for (const dir of SOURCE_DIRS) {
    for (const file of walk(join(ROOT, dir), [])) {
      readFileSync(file, "utf8").split(/\r?\n/).forEach((line, i) => {
        for (const { re, why } of FORBIDDEN) {
          if (re.test(line)) violations.push(`  ${relative(ROOT, file)}:${i + 1} — ${why}`);
        }
      });
    }
  }
  violations.forEach((v) => process.stdout.write(v + "\n"));
  return violations.length === 0;
}

function scanSecrets() {
  process.stdout.write("\n▶ Secretos (governance R6)\n");
  const violations = [];
  for (const dir of SECRET_DIRS) {
    for (const file of walk(join(ROOT, dir), [], SECRET_EXTS)) {
      for (const { line, kind } of findSecrets(readFileSync(file, "utf8"))) {
        violations.push(`  ${relative(ROOT, file)}:${line} — posible secreto (${kind})`);
      }
    }
  }
  violations.forEach((v) => process.stdout.write(v + "\n"));
  return violations.length === 0;
}

function uiScripts() {
  const pkg = join(ROOT, "ui", "package.json");
  if (!existsSync(pkg)) return null;
  return JSON.parse(readFileSync(pkg, "utf8")).scripts ?? {};
}

function reportError(error) {
  process.stdout.write(`\n✖ ${error}\n`);
  return false;
}

function buildSteps() {
  const planned = planSteps({
    hasCargo: existsSync(join(ROOT, "Cargo.toml")),
    uiScripts: uiScripts(),
    hasMemoryCli: existsSync(join(ROOT, "scripts", "memory.mjs")),
  });
  const steps = planned.map(({ label, cmd, error }) => [
    label,
    error ? () => reportError(error) : () => run(label, cmd),
  ]);
  steps.push(["Patrones prohibidos", scanForbiddenPatterns]);
  steps.push(["Secretos", scanSecrets]);
  return steps;
}

const results = buildSteps().map(([label, fn]) => [label, fn()]);
const failed = results.filter(([, ok]) => !ok).map(([label]) => label);

process.stdout.write("\n──── Quality gate ────\n");
results.forEach(([label, ok]) => process.stdout.write(`${ok ? "✔" : "✖"} ${label}\n`));

if (failed.length > 0) {
  process.stdout.write(`\nQUALITY GATE: ROJO (${failed.join(", ")})\n`);
  process.exit(1);
}
process.stdout.write("\nQUALITY GATE: VERDE\n");
