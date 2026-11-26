import { test } from "node:test";
import assert from "node:assert/strict";
import { checkPush } from "./push-guard.mjs";

const FEATURE = "fix/002-falsos-positivos-hooks";
const TO_MAIN = /main\/master/;
const FORCE = /forzado/;
const OPAQUE = /no se puede analizar/;
const R5 = /R5/;

const verdictOf = (command, { branch = FEATURE, tool = "Bash" } = {}) =>
  checkPush(command, { tool, currentBranch: () => branch });

function assertAllowed(commands, options) {
  for (const command of commands) assert.equal(verdictOf(command, options).allow, true, command);
}

function assertDenied(commands, reason, options) {
  for (const command of commands) {
    const verdict = verdictOf(command, options);
    assert.equal(verdict.allow, false, command);
    assert.match(verdict.reason, reason, command);
  }
}

test("push_caso_1_rama_de_feature", () =>
  assertAllowed(["cd /w/app-llama && git log --oneline main..HEAD && git push -u origin chore/001-memoria 2>&1 | tail -5"]));

test("push_main_fuera_del_push", () =>
  assertAllowed([
    "git diff origin/main && git push -u origin fix/002-x",
    "git switch main; git switch fix/002-x && git push origin fix/002-x",
    "git log master..HEAD | head -3; git push origin HEAD:refs/heads/fix/002-x",
  ]));

test("push_rama_con_main_en_el_nombre", () => assertAllowed(["git push origin feat/003-main-ui", "git push origin feat/004-domain"]));

test("push_sin_push", () =>
  assertAllowed(["git log --oneline main..HEAD", "echo main --force", "git stash push -m wip"], { branch: "main" }));

test("push_a_main_bloqueado", () =>
  assertDenied(
    [
      "git push origin main",
      "git push origin master",
      "git push origin Main",
      "git push origin HEAD:main",
      "git push origin @:main",
      "git push origin feat/x:main",
      "git push origin :main",
      "git push origin refs/heads/main",
      "git push origin HEAD:refs/heads/master",
      "git push --delete origin main",
      "git push origin feat/x main",
      "git push origin main~0:feat/x",
      "git push origin 'refs/heads/*'",
      "git push --all origin",
      "git push --mirror origin",
      "git -C . push origin main",
      "env GIT_TRACE=1 git push origin main",
      "cd x && git push origin main",
      'git push origin "main"',
      String.raw`git push origin ma\in`,
      "git push -o ci.skip origin main",
    ],
    TO_MAIN,
  ));

test("push_forzado_bloqueado", () =>
  assertDenied(
    [
      "git push --force origin feat/x",
      "git push -f origin feat/x",
      "git push -uf origin feat/x",
      "git push --force-with-lease origin feat/x",
      "git push --force-with-lease=feat/x origin feat/x",
      "git push --forc origin feat/x",
      "git push origin +feat/x",
      "git push origin feat/x --force",
      "git push origin +main",
    ],
    FORCE,
  ));

test("push_implicito_en_main_bloqueado", () => {
  for (const branch of ["main", "master", "", null]) {
    assertDenied(["git push", "git push origin", "git push -u origin HEAD", "git push origin @", "git push --tags"], TO_MAIN, {
      branch,
    });
  }
});

test("push_implicito_en_rama_permitido", () => assertAllowed(["git push", "git push -u origin HEAD"]));

test("push_explicito_no_consulta_la_rama", () => {
  const currentBranch = () => {
    throw new Error("no debe consultarse");
  };
  assert.equal(checkPush("git push origin feat/x", { tool: "Bash", currentBranch }).allow, true);
});

test("push_no_analizable_bloqueado", () =>
  assertDenied(
    ["git push origin $BRANCH", 'git push origin "$(git branch --show-current)"', "git push origin `git branch --show-current`"],
    OPAQUE,
  ));

test("push_configuracion_en_linea_bloqueada", () =>
  assertDenied(["git -c remote.origin.push=refs/heads/x:refs/heads/main push", "git -c alias.p=push p origin feat/x"], OPAQUE));

test("push_heredoc_de_datos", () =>
  assertAllowed(["cat > /tmp/b.md <<'EOF'\ngit push --force origin main\nEOF\ngh pr create --body-file /tmp/b.md"]));

test("push_powershell_regla_anterior", () => {
  const options = { tool: "PowerShell" };
  assertAllowed(["git push origin feat/x"], options);
  assertDenied(["git log main; git push origin feat/x"], TO_MAIN, options);
  assertDenied(["git push origin $branch"], OPAQUE, options);
});

test("push_tras_palabra_reservada", () => {
  assertDenied(["if git push origin main; then x; fi", "! git push origin main"], TO_MAIN);
  assertDenied(["time git push --force origin x"], FORCE);
});

test("push_abreviaturas_de_opciones", () =>
  assertDenied(["git push --mirr origin", "git push --al origin", "git push --branc origin", "git push --mirror origin"], TO_MAIN));

test("push_ref_dwim", () =>
  assertDenied(["git push origin x:heads/main", "git push origin heads/main", "git push origin refs/heads/main"], TO_MAIN));

test("push_git_anidado_u_oculto", () => {
  assertDenied(["bash -c 'git push origin main'", 'powershell -c "git push origin master"'], TO_MAIN);
  assertDenied(['sh -c "git push --force origin x"'], FORCE);
  assertDenied(["printf main | xargs git push origin", "xargs -a f git push origin"], OPAQUE);
  assertDenied(
    [
      "env -S'git push origin main'",
      "env --split-string='git push origin main'",
      'GIT_EDITOR="git push origin main;true" git commit --allow-empty',
      'git -c core.editor="git push origin main;true" commit --allow-empty',
      'git -c core.pager="git push origin main;" log',
      "echo main | xargs -I{} sh -c 'git push origin {}'",
      "/usr/lib/git-core/git-push origin main",
      "git-push.exe origin main",
      "git-push --force origin x",
      "time git-push origin main",
    ],
    R5,
  );
  assertAllowed(["git-push origin feat/x", "GIT_EDITOR=vim git commit", 'git commit -m "fix: arreglo"']);
});

test("push_config_en_linea_y_opciones_globales", () => {
  assertDenied(["git --attr-source HEAD push origin main"], TO_MAIN);
  assertDenied(["git --unknown-opt push origin x"], OPAQUE);
  assertDenied(["git -c alias.p='!git p\"u\"sh origin main' p", "GIT_CONFIG_COUNT=1 git p origin x"], OPAQUE);
  assertDenied(
    [
      'git -c alias.x=push rebase -x "git x origin main" HEAD~1',
      'git -c alias.x=push submodule foreach "git x origin main"',
    ],
    OPAQUE,
  );
  assertAllowed(["git -c core.x=y log","git -c user.name=x commit -m y", "git --no-pager log"]);
});

test("push_herestring_y_alias", () => {
  assertDenied(
    [
      "bash <<< 'git push origin main'",
      'sh -s <<<"git push -f origin feat/x"',
      "alias gp='git push'\ngp origin main",
    ],
    /main\/master|forzado|no se puede analizar/,
  );
  assertAllowed(["cat <<< 'hola'", "alias ll='ls -la'"]);
});

test("push_config_en_linea_sin_push_permitido", () =>
  assertAllowed([
    "git -c core.quotepath=off diff-tree -r HEAD",
    "git -c core.quotepath=off ls-remote origin",
    "git -c protocol.version=2 cat-file -p HEAD",
    "git -c x=y show-ref",
    "git -c color.ui=always log --grep=git",
  ]));

test("push_anidado_con_barra_invertida", () => assertDenied(["sh -c 'g\\it push origin main'"], TO_MAIN));
