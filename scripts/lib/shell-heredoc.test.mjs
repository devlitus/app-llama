import { test } from "node:test";
import assert from "node:assert/strict";
import { stripDataHeredocs } from "./shell-heredoc.mjs";

test("heredoc_cat_se_despoja", () => {
  const command =
    "cat > /tmp/pr.md <<'EOF'\nusa node scripts/memory.mjs y docs/memory/\nrm -rf docs/memory\nEOF\ngh pr create --body-file /tmp/pr.md";
  assert.equal(stripDataHeredocs(command), "cat > /tmp/pr.md  \ngh pr create --body-file /tmp/pr.md");
});

test("heredoc_comillas_dobles_y_guion", () =>
  assert.equal(stripDataHeredocs('cat <<-"FIN" > x\n\tdato\n\tFIN\nls'), "cat   > x\nls"));

test("heredoc_varios_cat", () =>
  assert.equal(stripDataHeredocs("cat > a <<'A'\nx\nA\ncat > b <<'B'\ny\nB"), "cat > a  \ncat > b  "));

test("heredoc_sin_heredoc_igual", () => assert.equal(stripDataHeredocs("ls -la"), "ls -la"));

test("heredoc_no_datos_intacto", () => {
  const commands = [
    "cat <<EOF\n$(rm -rf docs/memory)\nEOF",
    "bash <<'EOF'\nrm -rf docs/memory\nEOF",
    "cat <<'EOF' | sh\nrm x\nEOF",
    "cat <<'EOF' && sh\nrm x\nEOF",
    "cat <<'EOF'\nrm x",
    "cat >'<<'EOF''\nrm -rf docs/memory\nEOF",
    "echo 'a\ncat <<'EOF'\n' ; rm -rf docs/memory ; echo '\nEOF\n'",
    "cat <<'A' <<'B'\nx\nA\ny\nB",
    "cat <<'EOF'\r\nx\r\nEOF\r\n",
    "git apply <<'EOF'\n--- a/docs/memory/long/errores.md\nEOF",
  ];
  for (const command of commands) assert.equal(stripDataHeredocs(command), command, command);
});

test("heredoc_cabecera_sospechosa_intacta", () => {
  const commands = [
    "ls; cat <<'EOF'\nrm x\nEOF",
    "echo a # <<'EOF'\nrm x\nEOF",
    "echo 'a\ncat <<'EOF'\nrm x\nEOF",
    "ls \\\ncat <<'EOF'\nrm x\nEOF",
    "echo a |\ncat <<'EOF'\nrm x\nEOF",
    "cat <<<'EOF'\nrm x\nEOF",
    "cat <<'EOF'\nrm x\nEOF ",
    "cat <<'EOF'\nrm x\n\tEOF",
    "cat <<'EOF' |\necho EJECUTADO\nEOF\nsh",
    "cat <<'EOF' |&\necho EJECUTADO\nEOF\nsh",
    "cat <<'EOF' 2>&1 |\necho EJECUTADO\nEOF\nsh",
  ];
  for (const command of commands) assert.equal(stripDataHeredocs(command), command, command);
});
