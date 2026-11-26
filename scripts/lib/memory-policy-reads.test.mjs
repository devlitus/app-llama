import { test } from "node:test";
import assert from "node:assert/strict";
import { bash, developer, withRoot } from "./memory-policy-fixtures.mjs";
import { checkMemoryToolUse } from "./memory-policy.mjs";

const POWERSHELL = "PowerShell";
const verdictOf = (command, identity = developer, tool = "Bash") =>
  withRoot((root) => checkMemoryToolUse(bash(command, identity, tool), { root }));

function assertAllDenied(commands, identity = developer, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, false, command);
}

function assertAllAllowed(commands, identity = developer, tool = "Bash") {
  for (const command of commands) assert.equal(verdictOf(command, identity, tool).allow, true, command);
}

test("politica_node_test_permitido", () => {
  const command = "node --test scripts/lib/memory-policy.test.mjs";
  assertAllAllowed([command]);
  assertAllAllowed([command], {});
});

test("politica_node_test_opciones_peligrosas", () =>
  assertAllDenied([
    "node --test --import ./x.mjs scripts/lib/memory-policy.test.mjs",
    "node --test --test-reporter-destination=docs/memory/x scripts/lib/memory-policy.test.mjs",
    "node --test scripts/lib/memory-close.mjs",
  ]));

test("politica_grep_cli_permitido", () => assertAllAllowed(["grep node scripts/memory.mjs"]));

test("politica_lectura_tuberia", () => assertAllAllowed(["cat docs/memory/long/errores.md | grep x | wc -l"]));

test("politica_lectura_tuberia_a_escritura", () =>
  assertAllDenied(["cat docs/memory/long/x.md | tee docs/memory/long/y.md", "ls docs/memory | xargs rm"]));

test("politica_lectura_encadenada", () =>
  assertAllDenied(["cat docs/memory/long/x.md; rm x", "ls docs/memory || true", "ls docs/memory & rm x"]));

test("politica_lectura_sustitucion", () =>
  assertAllDenied(["cat $(echo docs/memory/long/x.md)", "cat `echo docs/memory/long/x.md`", 'cat "docs/memory/$F"', "cat docs/memory/$F"]));

test("politica_lectura_comillas_simples", () =>
  assertAllAllowed(["Select-String docs/memory/medium/001-memoria.md -Pattern '^estado: (abierto|cerrado)$'"], developer, POWERSHELL));

test("politica_lectura_comilla_escapada", () => assertAllDenied(["cat \\'$(rm x)\\' docs/memory/long/x.md"]));

test("politica_lectura_segmento_vacio", () => assertAllDenied(["ls docs/memory |", "| ls docs/memory"]));

test("politica_lectura_multilinea", () => assertAllDenied(["cat docs/memory/long/x.md\nrm x"]));

test("politica_find", () => assertAllAllowed(['find docs/memory -name "*.md"']));

test("politica_find_peligroso", () =>
  assertAllDenied(["find docs/memory -delete", "find docs/memory -exec rm {} +", "find docs/memory -fprint docs/memory/x"]));

test("politica_rg_pre", () => assertAllDenied(["rg --pre rm x docs/memory"]));

test("politica_git_lecturas", () =>
  assertAllAllowed([
    "git diff -- docs/memory",
    "git log docs/memory",
    "git show HEAD:docs/memory/long/errores.md",
    "git status docs/memory",
    "git blame docs/memory/long/errores.md",
    "git ls-files docs/memory/short",
  ]));

test("politica_git_opciones_peligrosas", () =>
  assertAllDenied([
    "git diff --output=docs/memory/long/x.md",
    "git -c core.pager=rm log docs/memory",
    "git grep -O x docs/memory",
  ]));

test("politica_git_add", () => assertAllAllowed(["git add docs/memory/medium/001-memoria.md"], {}));

test("politica_git_add_interactivo", () => assertAllDenied(["git add -p docs/memory", "git add --edit docs/memory"]));

test("politica_powershell_lecturas", () =>
  assertAllAllowed(
    [
      String.raw`Get-Content docs\memory\long\errores.md`,
      "gc docs/memory/long/errores.md",
      "Get-ChildItem docs/memory/short/*-001-memoria-*.md",
      "Get-Content docs/memory/long/errores.md | Measure-Object -Line",
    ],
    developer,
    POWERSHELL,
  ));

test("politica_powershell_subexpresion", () =>
  assertAllDenied(
    ["Get-Content (Remove-Item docs/memory/long/x.md)", "(Get-Content docs/memory/long/x.md).Count"],
    developer,
    POWERSHELL,
  ));

test("politica_less_fuera_de_lista", () => assertAllDenied(["less docs/memory/long/x.md"]));

test("politica_lectura_comilla_sin_cerrar", () =>
  assertAllDenied(['cat "docs/memory/long/x.md', "cat 'docs/memory/long/x.md"]));

test("politica_git_grep_pager", () =>
  assertAllDenied([
    'git grep "-Orm -v" x docs/memory',
    "git grep -O/bin/rm x docs/memory",
    "git grep -iO/bin/rm x docs/memory",
    "git grep --open=rm x docs/memory",
    "git grep --open-files-in=rm x docs/memory",
  ]));

test("politica_git_add_force", () =>
  assertAllDenied(["git add -f docs/memory/short/x.md", "git add --forc docs/memory/x", "git add --force docs/memory/x"]));

test("politica_git_abreviatura_ext_diff", () => assertAllDenied(["git diff --ext docs/memory"]));

test("politica_git_grep_opciones_inocuas", () =>
  assertAllAllowed(["git grep -i x docs/memory", "git grep -n x docs/memory", "git diff --stat docs/memory"]));

test("politica_lectura_redireccion", () => assertAllDenied(["cat docs/memory/long/x.md > docs/memory/long/y.md"]));

test("politica_lectura_bloque_de_script", () =>
  assertAllDenied(["Select-Object -InputObject docs/memory/x -Property {Remove-Item x}"], developer, POWERSHELL));

test("politica_find_fls", () => assertAllDenied(["find docs/memory -fls out"]));

test("politica_git_diff_ext_diff", () => assertAllDenied(["git diff --ext-diff docs/memory"]));

test("politica_git_grep_open_files_in_pager", () =>
  assertAllDenied(["git grep --open-files-in-pager=less x docs/memory"]));

test("politica_git_add_patch", () => assertAllDenied(["git add --patch docs/memory/x"]));

test("politica_node_import_sin_test", () =>
  assertAllDenied(["node --import=./x.mjs scripts/lib/memory-policy.test.mjs"]));

test("politica_lectura_llaves_sueltas", () =>
  assertAllDenied(["cat docs/memory/long/x.md {a", "cat docs/memory/long/x.md a}"]));

test("politica_lectura_entrada_redirigida", () => assertAllDenied(["cat < docs/memory/long/x.md"]));
