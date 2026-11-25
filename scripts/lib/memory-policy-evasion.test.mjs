import { test } from "node:test";
import assert from "node:assert/strict";
import { bash, developer, reviewer, withRoot } from "./memory-policy-fixtures.mjs";
import { checkMemoryToolUse, memoryCommandLine } from "./memory-policy.mjs";

const POWERSHELL = "PowerShell";

function verdictOf(command, identity = developer, tool = "Bash") {
  return withRoot((root) => checkMemoryToolUse(bash(command, identity, tool), { root }));
}

function assertAllDenied(commands, identity = developer, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, false, command);
}

function assertAllAllowed(commands, identity = developer, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, true, command);
}

test("politica_10_touch_truncate_unlink", () =>
  assertAllDenied([
    "touch docs/memory/short/x.md",
    "truncate -s 0 docs/memory/long/errores.md",
    "unlink docs/memory/long/errores.md",
  ]));

test("politica_10_dd", () => assertAllDenied(["dd if=/dev/null of=docs/memory/long/errores.md"]));

test("politica_10_perl_pi", () => {
  const result = verdictOf("perl -pi -e 's/a/b/' docs/memory/long/errores.md");
  assert.equal(result.allow, false);
  assert.match(result.reason, /código en línea/);
});

test("politica_10_node_e", () => {
  const result = verdictOf(`node -e "require('fs').writeFileSync('docs/memory/long/x.md','x')"`);
  assert.equal(result.allow, false);
  assert.match(result.reason, /código en línea/);
});

test("politica_10_python_c", () => assertAllDenied([`python -c "open('docs/memory/long/x.md','w').write('x')"`]));

test("politica_10_git_clean_stash_reset", () =>
  assertAllDenied([
    "git clean -fdX docs/memory",
    "git stash push -- docs/memory",
    "git reset --hard HEAD -- docs/memory",
  ]));

test("politica_10_alias_powershell", () =>
  assertAllDenied(
    [String.raw`del docs\memory\long\x.md`, "ri docs/memory -Recurse", "ni docs/memory/short/x.md"],
    developer,
    POWERSHELL,
  ));

test("politica_10_dotnet", () =>
  assertAllDenied([`[IO.File]::WriteAllText('docs/memory/long/x.md','x')`], developer, POWERSHELL));

test("politica_10_barras_dobles_y_punto", () =>
  assertAllDenied([
    "rm docs//memory/long/x.md",
    "rm docs/./memory/long/x.md",
    "rm docs/x/../memory/long/x.md",
  ]));

test("politica_10_comillas_y_escapes", () =>
  assertAllDenied([`rm -rf "docs"/'memory'`, String.raw`rm -rf do\cs/memory`]));

test("politica_10_cd_docs", () => assertAllDenied(["cd docs && rm -rf memory/short"]));

test("politica_10_glob", () =>
  assertAllDenied([
    "rm -rf docs/mem*",
    "rm -rf docs/*",
    "rm -rf d*/memory",
    "rm docs/{memory,x}/long/x.md",
  ]));

test("politica_10_variable", () => assertAllDenied(["D=docs/mem; rm -rf ${D}ory", "D=do; rm -rf ${D}cs/memory"]));

test("politica_10_prefijo_parcial", () => assertAllDenied(["rm docs/mem", "rm docs/me", "rm docs/m"]));

test("politica_10_variable_tras_docs", () => assertAllDenied(["rm -rf docs/$X", "rm -rf %D%/docs"]));

test("politica_11_import_desde_lib", () =>
  assertAllDenied([`cd scripts/lib && node -e "import('./memory-close.mjs').then((m) => m.close())"`], reviewer));

test("politica_11_variable_en_node", () => {
  const result = verdictOf("${n}de scripts/memory.mjs close", reviewer);
  assert.equal(result.allow, false);
  assert.match(result.reason, /forma de comando no admitida/);
});

test("politica_11_otros_runtimes", () =>
  assertAllDenied(
    [
      "nodejs scripts/memory.mjs close",
      "bun scripts/memory.mjs close",
      "deno run scripts/memory.mjs close",
      "npx tsx scripts/memory.mjs close",
    ],
    reviewer,
  ));

const COMMAND_AFTER_EOF =
  "node scripts/memory.mjs session --agent developer --agent-id a1 --task T2 <<'EOF'\nresumen: x\nEOF\nrm -rf docs/memory";

test("politica_heredoc_con_ordenes_tras_eof", () => assert.equal(verdictOf(COMMAND_AFTER_EOF).allow, false));

test("linea_comando_ordenes_tras_eof", () => assert.equal(memoryCommandLine(COMMAND_AFTER_EOF), null));

test("politica_remove_item_independiente", () => {
  const result = verdictOf("Remove-Item docs/memory/long/x.md", developer, POWERSHELL);
  assert.equal(result.allow, false);
  assert.match(result.reason, /la memoria solo se escribe/);
});

test("politica_cp_mayusculas", () => assertAllDenied(["cp x DOCS/MEMORY/long/errores.md"]));

test("politica_iex", () => {
  const result = verdictOf('iex "Remove-Item docs/memory -Recurse"', developer, POWERSHELL);
  assert.equal(result.allow, false);
  assert.match(result.reason, /código en línea/);
});

test("politica_interprete_aunque_lea", () =>
  assertAllDenied([
    `python -c "print(open('docs/memory/long/x.md').read())"`,
    `sh -c "cat docs/memory/long/x.md"`,
    `pwsh -Command "Get-Content docs/memory/long/x.md"`,
  ]));

test("politica_mensaje_commit_no_es_mencion", () => assertAllAllowed([`git commit -m "docs(memory): x"`]));

test("politica_otras_rutas_de_docs", () => assertAllAllowed(["rm docs/plans/002-x.md", "rm -rf docs/memoryx"]));
