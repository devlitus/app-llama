import test from "node:test";
import assert from "node:assert/strict";
import { planSteps } from "./gate-steps.mjs";

const NONE = { hasCargo: false, uiScripts: null, hasMemoryCli: false };
const labels = (steps) => steps.map((s) => s.label);

test("pasos_repo_vacio", () => {
  assert.deepEqual(planSteps(NONE), []);
});

test("pasos_solo_scripts", () => {
  const steps = planSteps({ ...NONE, hasMemoryCli: true });
  assert.deepEqual(labels(steps), ["Tests scripts", "Memoria (R11)"]);
});

test("pasos_solo_rust", () => {
  const steps = planSteps({ ...NONE, hasCargo: true });
  assert.deepEqual(steps, [
    { label: "Formato Rust", cmd: "cargo fmt --all -- --check" },
    { label: "Lint Rust", cmd: "cargo clippy --workspace --all-targets --locked -- -D warnings" },
    { label: "Tests Rust", cmd: "cargo test --workspace --locked" },
  ]);
});

test("pasos_orden_completo", () => {
  const steps = planSteps({
    hasCargo: true,
    uiScripts: { build: "x", lint: "x", check: "x", test: "x" },
    hasMemoryCli: true,
  });
  assert.deepEqual(labels(steps), [
    "Build UI",
    "Formato Rust",
    "Lint Rust",
    "Tests Rust",
    "Lint UI",
    "Tipos UI",
    "Tests UI",
    "Tests scripts",
    "Memoria (R11)",
  ]);
});

test("pasos_ui_sin_build", () => {
  const steps = planSteps({ ...NONE, uiScripts: { lint: "x", test: "x" } });
  assert.deepEqual(labels(steps), ["Lint UI", "Tests UI"]);
});

test("pasos_ui_falta_lint_y_test", () => {
  const steps = planSteps({ ...NONE, uiScripts: {} });
  assert.equal(steps.length, 2);
  assert.match(steps[0].error, /'lint'/);
  assert.match(steps[1].error, /'test'/);
  assert.ok(steps.every((s) => !("cmd" in s)));
});

test("pasos_build_ui_comando", () => {
  const steps = planSteps({ ...NONE, uiScripts: { build: "x", lint: "x", test: "x" } });
  assert.deepEqual(steps[0], { label: "Build UI", cmd: "npm --prefix ui run build" });
});
