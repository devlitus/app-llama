import { test } from "node:test";
import assert from "node:assert/strict";
import { pushVerdict } from "./push-hook.mjs";

const currentBranch = () => "fix/002-falsos-positivos-hooks";
const withoutModule = () => Promise.reject(new Error("módulo ausente"));

test("hook_push_excepcion_bloquea", async () => {
  const loadGuard = () =>
    Promise.resolve({
      checkPush: () => {
        throw new Error("fallo interno");
      },
    });
  const verdict = await pushVerdict("git push origin feat/x", { tool: "Bash", loadGuard, currentBranch });
  assert.equal(verdict.allow, false);
  assert.match(verdict.reason, /fallo interno/);
});

test("hook_push_reserva_sin_modulo", async () => {
  const options = { tool: "Bash", loadGuard: withoutModule, currentBranch };
  assert.equal((await pushVerdict("git push origin main", options)).allow, false);
  assert.equal((await pushVerdict("git push --force origin x", options)).allow, false);
  assert.equal((await pushVerdict("git push origin MAIN", options)).allow, false);
  assert.equal((await pushVerdict(String.raw`git push origin ma\in`, options)).allow, false);
  assert.equal((await pushVerdict("git push origin $BRANCH", options)).allow, false);
  assert.equal((await pushVerdict("git push origin feat/x", options)).allow, true);
  assert.equal((await pushVerdict("git log main", options)).allow, true);
});

test("hook_push_usa_el_analizador", async () => {
  const loadGuard = () => Promise.resolve({ checkPush: (command, context) => ({ allow: true, command, context }) });
  const verdict = await pushVerdict("git push", { tool: "Bash", loadGuard, currentBranch });
  assert.equal(verdict.command, "git push");
  assert.equal(verdict.context.tool, "Bash");
});
