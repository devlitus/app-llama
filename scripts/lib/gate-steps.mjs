// Plan puro de los pasos externos del quality gate: sin acceso a disco ni ejecución.

const RUST_STEPS = [
  { label: "Formato Rust", cmd: "cargo fmt --all -- --check" },
  { label: "Lint Rust", cmd: "cargo clippy --workspace --all-targets --locked -- -D warnings" },
  { label: "Tests Rust", cmd: "cargo test --workspace --locked" },
];

const REQUIRED_UI_SCRIPTS = ["lint", "test"];

function missingScriptSteps(uiScripts) {
  return REQUIRED_UI_SCRIPTS.filter((name) => !uiScripts[name]).map((name) => ({
    label: `Script UI '${name}'`,
    error: `ui/package.json debe definir el script '${name}'`,
  }));
}

function uiSteps(uiScripts) {
  const steps = [...missingScriptSteps(uiScripts)];
  if (uiScripts.lint) steps.push({ label: "Lint UI", cmd: "npm --prefix ui run lint" });
  if (uiScripts.check) steps.push({ label: "Tipos UI", cmd: "npm --prefix ui run check" });
  if (uiScripts.test) steps.push({ label: "Tests UI", cmd: "npm --prefix ui test" });
  return steps;
}

export function planSteps({ hasCargo, uiScripts, hasMemoryCli }) {
  const steps = [];
  // Va antes de Rust porque src-tauri incrusta ui/dist al compilar.
  if (uiScripts?.build) steps.push({ label: "Build UI", cmd: "npm --prefix ui run build" });
  if (hasCargo) steps.push(...RUST_STEPS);
  if (uiScripts) steps.push(...uiSteps(uiScripts));
  if (hasMemoryCli) {
    steps.push({ label: "Tests scripts", cmd: 'node --test "scripts/**/*.test.mjs"' });
    steps.push({ label: "Memoria (R11)", cmd: "node scripts/memory.mjs check" });
  }
  return steps;
}
