import { test } from "node:test";
import assert from "node:assert/strict";
import { SECRET_PATTERNS, findSecrets } from "./secrets.mjs";

const hfToken = "hf_" + "A".repeat(34);

test("secrets_detecta_token_hf", () => {
  assert.deepEqual(findSecrets("token = " + hfToken), [{ line: 1, kind: "hugging-face" }]);
});

test("secrets_ignora_hf_corto", () => {
  assert.deepEqual(findSecrets("hf_abc"), []);
});

test("secrets_detecta_github", () => {
  assert.deepEqual(findSecrets("ghp_" + "a".repeat(36)), [{ line: 1, kind: "github" }]);
});

test("secrets_detecta_github_pat", () => {
  assert.deepEqual(findSecrets("github_pat_" + "a".repeat(22)), [{ line: 1, kind: "github" }]);
});

test("secrets_detecta_anthropic", () => {
  assert.deepEqual(findSecrets("sk-ant-" + "a".repeat(24)), [{ line: 1, kind: "anthropic" }]);
});

test("secrets_detecta_aws", () => {
  assert.deepEqual(findSecrets("AKIA" + "ABCDEFGHIJKLMNOP"), [{ line: 1, kind: "aws" }]);
});

test("secrets_detecta_clave_privada", () => {
  const dashes = "-".repeat(5);
  for (const header of ["RSA PRIVATE KEY", "PRIVATE KEY"]) {
    assert.deepEqual(findSecrets(`${dashes}BEGIN ${header}${dashes}`), [
      { line: 1, kind: "private-key" },
    ]);
  }
});

test("secrets_numera_lineas_crlf", () => {
  assert.deepEqual(findSecrets("a\r\nb\n" + hfToken), [{ line: 3, kind: "hugging-face" }]);
});

test("secrets_dos_en_una_linea", () => {
  const text = "x " + "AKIA" + "ABCDEFGHIJKLMNOP" + " " + hfToken;
  assert.deepEqual(findSecrets(text), [
    { line: 1, kind: "hugging-face" },
    { line: 1, kind: "aws" },
  ]);
});

test("secrets_texto_limpio", () => {
  assert.deepEqual(findSecrets("Guarda el token de HF en keyring"), []);
});

test("secrets_no_se_detecta_a_si_mismo", () => {
  const source = SECRET_PATTERNS.map((p) => String(p.re)).join("\n");
  assert.deepEqual(findSecrets(source), []);
});
