import { test } from "node:test";
import assert from "node:assert/strict";
import { parseCommandList } from "./shell-words.mjs";

const wordsOf = (command) => parseCommandList(command)?.map(({ words }) => words);
const redirectsOf = (command) => parseCommandList(command)?.map(({ redirects }) => redirects);

test("palabras_orden_simple", () =>
  assert.deepEqual(wordsOf("git push -u origin feat/x"), [["git", "push", "-u", "origin", "feat/x"]]));

test("palabras_separadores", () =>
  assert.deepEqual(wordsOf("a 1 && b; c || d | e & f\ng"), [["a", "1"], ["b"], ["c"], ["d"], ["e"], ["f"], ["g"]]));

test("palabras_comillas", () => assert.deepEqual(wordsOf(`echo 'a; b' "c | d" e"f"g`), [["echo", "a; b", "c | d", "efg"]]));

test("palabras_escape_bash", () =>
  assert.deepEqual(wordsOf(String.raw`git push origin ma\in`), [["git", "push", "origin", "main"]]));

test("palabras_continuacion_de_linea", () => assert.deepEqual(wordsOf("ls \\\n-la"), [["ls", "-la"]]));

test("palabras_placeholder_find", () =>
  assert.deepEqual(wordsOf(String.raw`find . -exec rm {} \;`), [["find", ".", "-exec", "rm", "{}", ";"]]));

test("palabras_redirecciones", () =>
  assert.deepEqual(redirectsOf("cat a > b 2>&1 >> c &> d 2> e < f"), [
    [
      { op: ">", target: "b" },
      { op: "2>&", target: "1" },
      { op: ">>", target: "c" },
      { op: "&>", target: "d" },
      { op: "2>", target: "e" },
      { op: "<", target: "f" },
    ],
  ]));

test("palabras_vacia", () => {
  assert.deepEqual(parseCommandList(""), []);
  assert.deepEqual(wordsOf("a;;b"), [["a"], ["b"]]);
});

test("palabras_no_admitidas", () => {
  const commands = [
    "echo $HOME", "echo `id`", "echo $(id)", "(cd x)", "{ ls; }", 'echo "a $b"', "echo 'a", 'echo "a',
    String.raw`echo \'`, String.raw`echo "a\"b"`, "cat <<EOF", "a\r\nb", "cat > ", "ls >", "echo > 2>x",
  ];
  for (const command of commands) assert.equal(parseCommandList(command), null, command);
});

test("palabras_comentario_no_admitido", () => {
  for (const command of ["echo a #'\nls x\n#'", "echo a # x \\nls x", "# c\nls"]) {
    assert.equal(parseCommandList(command), null, command);
  }
  assert.deepEqual(wordsOf("a#b"), [["a#b"]]);
});

test("palabras_comodines_no_admitidos", () => {
  for (const command of ["ls ma[i]n", "ls ma?n", "ls *.md"]) {
    assert.equal(parseCommandList(command), null, command);
  }
  assert.deepEqual(wordsOf("ls 'a*'"), [["ls", "a*"]]);
});

test("palabras_ampersand_redireccion_sin_descriptor", () => {
  assert.deepEqual(wordsOf("echo 2&>x"), [["echo", "2"]]);
  assert.deepEqual(redirectsOf("echo 2&>x"), [[{ op: "&>", target: "x" }]]);
});
