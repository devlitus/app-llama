// Stop: si hay cambios en código fuente, no deja terminar el turno con el gate en rojo. R1.

import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { readHookInput, git, PROJECT_DIR } from "./lib.mjs";

const input = readHookInput();
if (input.stop_hook_active) process.exit(0); // evita bucles: un solo reintento por turno

const status = git(["status", "--porcelain"]);
if (!status) process.exit(0);

const SOURCE = /^(crates\/|src-tauri\/|ui\/|Cargo\.(toml|lock)$)/;
const changed = status
  .split("\n")
  .map((line) => line.slice(3).replace(/^"|"$/g, ""))
  .some((path) => SOURCE.test(path));
if (!changed) process.exit(0);

const res = spawnSync("node", [join(PROJECT_DIR, "scripts", "quality-gate.mjs")], {
  cwd: PROJECT_DIR,
  encoding: "utf8",
});
if (res.status === 0) process.exit(0);

const tail = `${res.stdout}\n${res.stderr}`.trim().split("\n").slice(-60).join("\n");
process.stderr.write(
  `Hay cambios en código y el quality gate está en ROJO. Arréglalo antes de terminar (R1).\n\n${tail}\n`,
);
process.exit(2);
