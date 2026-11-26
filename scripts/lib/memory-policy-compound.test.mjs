import { test } from "node:test";
import assert from "node:assert/strict";
import { bash, developer, reviewer, withRoot } from "./memory-policy-fixtures.mjs";
import { checkMemoryToolUse } from "./memory-policy.mjs";
import { splitCommandList } from "./memory-shell-syntax.mjs";

const ORCHESTRATOR = {};
const verdictOf = (command, identity = ORCHESTRATOR, tool = "Bash") =>
  withRoot((root) => checkMemoryToolUse(bash(command, identity, tool), { root }));

function assertAllAllowed(commands, identity = ORCHESTRATOR, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, true, command);
}

function assertAllDenied(commands, identity = ORCHESTRATOR, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, false, command);
}

const CASE_2_HEREDOC = [
  "cat > /tmp/scratch/pr-body.md <<'EOF'",
  "Cierre con `node scripts/memory.mjs close` y docs/memory/medium/001-memoria.md",
  "EOF",
  'gh pr create --title "chore(memoria): cierre" --body-file /tmp/scratch/pr-body.md',
].join("\n");

test("lista_separadores", () =>
  assert.deepEqual(splitCommandList("a; b && c || d\ne | f & g"), ["a", "b", "c", "d", "e | f & g"]));

test("lista_comillas", () =>
  assert.deepEqual(splitCommandList(`grep "a; b" x && echo 'c || d'`), [`grep "a; b" x`, `echo 'c || d'`]));

test("lista_no_admitida", () => {
  assert.equal(splitCommandList('echo "a'), null);
  assert.equal(splitCommandList(String.raw`echo \'a`), null);
});

test("politica_caso_2_heredoc_a_scratchpad", () => {
  assertAllAllowed([CASE_2_HEREDOC]);
  assertAllAllowed([CASE_2_HEREDOC], developer);
});

test("politica_caso_2_compuesto_con_check", () =>
  assertAllAllowed([
    'cd /w/app-llama && grep -n "Estado" docs/plans/001-memoria.md; ls docs/memory/medium/; grep -n "estado" docs/memory/medium/001-memoria.md; node scripts/memory.mjs check; git status -s',
  ]));

test("politica_caso_3_compuesto_lectura", () =>
  assertAllAllowed([
    'grep -rn "estado" docs/plans/001-memoria.md ; ls docs/memory/medium/; grep -n "estado" docs/memory/medium/001-memoria.md | head; git log --oneline -3; git status -s',
  ]));

test("politica_compuesto_redirecciones_inocuas", () =>
  assertAllAllowed(["ls docs/memory/medium/ 2>/dev/null; cat docs/memory/long/errores.md 2>&1 | head -5"]));

test("politica_cd_y_pwd", () => assertAllAllowed(["cd docs/memory && ls && pwd", "ls docs/memory;"]));

test("politica_compuesto_con_escritura", () => {
  const commands = [
    "ls docs/memory; rm -rf docs/memory/short",
    "cd docs/memory && touch x",
    "node scripts/memory.mjs check; node scripts/memory.mjs close",
    "ls docs/memory 2>docs/memory/x",
    "cat docs/memory/long/x.md 2>&1> docs/memory/y",
    "ls docs/memory; echo x > docs/memory/long/y.md",
    "cd docs/memory; cat x | tee y",
    "cd docs/memory && cd\nrm x",
  ];
  assertAllDenied(commands);
  assertAllDenied(commands, reviewer);
});

test("politica_heredoc_cat_hacia_memoria", () => {
  const result = verdictOf("cat > docs/memory/long/errores.md <<'EOF'\nx\nEOF");
  assert.equal(result.allow, false);
  assert.match(result.reason, /la memoria solo se escribe/);
});

test("politica_heredoc_no_datos", () =>
  assertAllDenied([
    "bash <<'EOF'\nrm -rf docs/memory\nEOF",
    "cat <<'EOF' | sh\nrm -rf docs/memory\nEOF",
    "cat <<EOF\n$(rm -rf docs/memory)\nEOF",
    "cat <<'EOF' > /tmp/x.md\nrm docs/memory/x\nEOF\nrm -rf docs/memory",
  ]));

test("politica_heredoc_powershell_sin_despojar", () => assertAllDenied([CASE_2_HEREDOC], ORCHESTRATOR, "PowerShell"));

test("politica_orden_constructor", () => assertAllDenied(["constructor docs/memory", "toString docs/memory"]));

test("politica_varios_heredocs_de_datos", () => {
  const body = "Ver docs/memory/ y cerrar con `node scripts/memory.mjs close`";
  const command = `cat > /tmp/a.md <<'EOF'\n${body}\nEOF\ncat > /tmp/b.md <<'EOF'\n${body}\nEOF`;
  assertAllAllowed([command]);
  assertAllAllowed([command], developer);
});
