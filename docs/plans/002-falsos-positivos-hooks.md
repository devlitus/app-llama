# 002 — Falsos positivos de los hooks de Bash y retirada de R8

- **Tipo:** bug
- **Estado:** en curso
- **Rama:** fix/002-falsos-positivos-hooks
- **Fase del roadmap:** previa a la fase 0 (Esqueleto) de [docs/MVP.md](../MVP.md), igual que los planes 000 y 001

## Objetivo

Que el equipo pueda encadenar lecturas, citar rutas dentro de datos (heredocs, textos) y hacer push a su rama sin que los hooks lo bloqueen, y que el código y la documentación reflejen que R8 (ficheros protegidos) se ha retirado. Los hooks siguen bloqueando el push a `main`/`master`, el push forzado (R5) y las escrituras en `docs/memory/` fuera de `node scripts/memory.mjs` (R11). Ante la duda, bloquean.

## Contexto

### Decisión del usuario: se retira R8

El usuario ya ha vaciado a mano `PROTECTED_PATHS` en `.claude/hooks/lib.mjs` (`export const PROTECTED_PATHS = [\n\n];`). A partir de ahora los agentes pueden editar `.claude/hooks/**`, `scripts/lib/**`, `scripts/quality-gate.mjs`, `scripts/memory.mjs`, `docs/GOVERNANCE.md`, etc. Este plan deja coherentes el código, los agentes y la governance con esa decisión. **R5 y R11 siguen vigentes.** Por eso todas las tareas son `(developer)` con TDD normal. Los planes cerrados 000 y 001 no se tocan.

Referencias a R8 que quedan fuera de los planes cerrados (grep `R8|PROTECTED_PATHS|isProtected|ficheros de governance`):
- `.claude/hooks/lib.mjs`: `PROTECTED_PATHS`, `toProjectRelative` y `isProtected`. Solo los usa `protect-files.mjs`.
- `.claude/hooks/protect-files.mjs`: L2, L4, L8 y L11-28 (bloque R8 de ficheros y de órdenes).
- `scripts/lib/memory-policy.mjs`: L21-24. `memoryCommandLine` solo existe para R8 (la usa `protect-files.mjs`) y quedará como código muerto (R7). Sus tests son `linea_comando_memoria` y `linea_comando_ajeno` (`memory-policy.test.mjs`) y `linea_comando_ordenes_tras_eof` (`memory-policy-evasion.test.mjs`).
- `docs/GOVERNANCE.md`: §1 (tabla, "editar ficheros de governance") y §3 R8.
- `.claude/agents/developer.md`: L22.

### Causa raíz de cada caso

| # | Hook y mensaje | Orden (abreviada) | Causa raíz | Cómo se resuelve |
|---|---|---|---|---|
| 1 | `guard-shell.mjs` — "push directo a main/master (R5)" | `cd … && git log --oneline main..HEAD && git push -u origin chore/001-memoria 2>&1 \| tail -5` | L26-33: `isGit("push")` solo detecta que hay un push. Después aplica `/\b(main\|master)\b/` y la regex de fuerza a **toda** la orden, no a los argumentos del push. `main..HEAD` del `git log` dispara el bloqueo. El `\b` también bloquea ramas como `feat/003-main-ui`, y `\s-f\b` salta con un `rm -f` o un `tail -f` en otra parte. | T4 |
| 2a | `protect-files.mjs` (R11) — "forma de comando no admitida" | `cat > <scratchpad>/pr.md <<'EOF'` + texto que nombra `scripts/memory.mjs` y `docs/memory/` + `EOF` + `gh pr create --body-file …` | `memory-policy.mjs` `checkShellCommand`: si la orden no es una invocación exacta de `memory.mjs`, `touchesMemory` analiza **el texto entero, cuerpo del heredoc incluido**. Después `isAllowedRead` → `splitPipeline` rechaza los saltos de línea, el `>` y el `<`. `mentionsCli` ve `memory.mjs` y elige el mensaje "forma no admitida". | T3 + T5 |
| 2b | Igual que 2a | `cd … && grep … docs/plans/001-memoria.md; ls docs/memory/medium/; grep -n "estado" …; node scripts/memory.mjs check; …` | `memory-readonly.mjs` `isAllowedRead` solo admite **una** tubería: `splitPipeline` devuelve `null` con `;` y `&`, así que `&&` y `;` niegan siempre. Además `cd` no está en la lista, y `node scripts/memory.mjs check` solo se acepta como orden completa, nunca como un segmento. | T5 |
| 3 | `protect-files.mjs` (R11) — "la memoria solo se escribe con…" | `grep … ; ls docs/memory/medium/; grep -n "estado" … \| head; git log …; git status -s` | La misma causa que 2b. Como no se menciona `memory.mjs`, el mensaje es `ONLY_VIA_CLI`. | T5 |
| 4 | `protect-files.mjs` (R8) — "modifica ficheros de governance" | `ls docs/plans/ .claude/hooks/; git ls-tree … .claude/hooks/; git fetch -q && git log --oneline -1 origin/main` | L20-27: `writes` (`>`, `rm`, `cp`…) y `touchesProtected` se evalúan por separado sobre toda la orden, así que un `>` cualquiera (`2>&1`, `2>/dev/null`, `->`) y una mención protegida en cualquier otra parte bastan para bloquear. Con el texto resumido no aparece ninguna señal de escritura: la orden real probablemente llevaba una redirección. | Desaparece con la retirada de R8 (T1). El e2e lo comprueba. |

Al investigar también apareció que **un hook que lanza una excepción sale con código 1**, y Claude Code lo trata como error **no bloqueante** (fail-open). Ejemplo: `constructor docs/memory` lanza un `TypeError` en `memory-readonly.mjs`, porque `"constructor" in FORBIDDEN_OPTIONS` es `true`. Se corrige en T1 (`safely`) y en T5 (`Object.hasOwn`).

### Diseño

1. **Analizador léxico de Bash** (`scripts/lib/shell-words.mjs`, `parseCommandList(command)`). Parte la orden en órdenes simples `{ words, redirects }` separadas por `;`, `&&`, `||`, `|`, `&` y saltos de línea:
   - Comillas simples: literales. Comillas dobles: solo sin `$`, comillas invertidas ni `\` dentro.
   - `\x` se convierte en `x`, y `\` + salto de línea continúa la línea.
   - Redirecciones: `>`, `>>`, `>|`, `>&`, `&>`, `&>>`, `<`, `<<<`, `<&`, `<>`, con prefijo de descriptor (`2>&1` → `{ op: "2>&", target: "1" }`).
   - `{}` es literal (para `find`).
   - **Devuelve `null`** ante `$`, comillas invertidas, `(`, `)`, `{`, `}`, `\r`, `<<` (heredoc no despojado), comillas sin cerrar o escapadas, y redirecciones sin destino. Con `null`, el llamador aplica su regla conservadora.
2. **Heredocs de datos** (`scripts/lib/shell-heredoc.mjs`, `stripDataHeredocs(command)`). Quita el cuerpo de `cat <<'TAG'` o `cat <<"TAG"` (también con `<<-`) solo cuando:
   - el `<<` está fuera de comillas;
   - `cat` va solo, sin argumentos ni tuberías, aunque puede llevar redirecciones (esas siguen evaluándose);
   - el texto anterior a la línea se analiza sin error;
   - y el terminador existe.

   En cualquier otro caso devuelve la orden **intacta**: sin comillas en el delimitador, consumido por un intérprete, por una tubería o por `git apply`, dos heredocs en una línea o CRLF.
3. **R5** (`scripts/lib/push-guard.mjs`, `checkPush(command, { tool, currentBranch })`):
   - Con Bash se despojan los heredocs y se analiza la orden. Por cada orden con una palabra `git` (también precedida de `env`, `xargs`…), el subcomando se localiza saltando las opciones globales (`-C`, `-c`, `--git-dir`, `--work-tree`, `--namespace` y `--config-env` llevan valor). **Solo se evalúan los argumentos del `push`.**
   - **Fuerza:** `--force`, `--force-with-lease`, `--force-if-includes` y sus abreviaturas de 4 o más caracteres; `-f`, también agrupado (`-uf`); un refspec con `+`.
   - **main/master:** algún lado de algún refspec (o el remoto) es `main`/`master`, sin distinguir mayúsculas y tras quitar `refs/heads/`, `refs/remotes/<r>/`, `~n` y `^`. También el comodín `*` y `--all`, `--mirror` o `--branches`. Las opciones con valor aparte (`-o`, `--push-option`, `--repo`, `--receive-pack`, `--exec`) se saltan.
   - **Implícito:** sin refspec, o con `HEAD` o `@`, se consulta `currentBranch()`. Se bloquea si es `main`, `master`, `""` o `null`. Con refspec explícito **no se consulta**.
   - **Opaco:** `-c` o `--config-env` antes del subcomando en un git que mencione `push`. Si la orden no se puede analizar (o la herramienta es PowerShell), se aplica `legacyPushVerdict`: la regla anterior más el bloqueo cuando el tramo del push contiene `$` o comillas invertidas.
   - En `guard-shell.mjs`, un import dinámico con reserva a la regla anterior y un `try/catch` que bloquea si hay una excepción.
4. **R11** (`scripts/lib/memory-*.mjs`):
   - `splitCommandList` parte la orden por `;`, `&&`, `||` y saltos de línea fuera de comillas.
   - Cada elemento debe ser una tubería de lecturas permitidas. Ahora son lecturas también `cd` con un argumento como máximo, `pwd` y `node scripts/memory.mjs check|tags|help`. Se quitan antes `2>&1`, `2>/dev/null` y `>/dev/null`.
   - En Bash se despojan antes los heredocs de datos.
   - `Object.hasOwn` en lugar de `in`.
   - Escribir sigue siendo posible solo con la forma exacta de `memory.mjs`.

### Casos ambiguos que se siguen bloqueando a propósito

| Caso | Motivo | Alternativa |
|---|---|---|
| `git commit -m "$(cat <<'EOF' … EOF)"` con `main` o `--force` junto a `git push` en la misma línea | `$(…)` no se analiza → regla anterior de R5 | `git commit -F <fichero del scratchpad>` |
| Push con `$BRANCH`, `$(…)` o comillas invertidas; `git -c … push`; `git -c alias.x=push x …` | La rama no se puede comprobar | Escribir la rama literal |
| `git push`, `git push origin` o `git push origin HEAD` en `main`, en HEAD separado o si `git` falla | Destino implícito no verificable o protegido | Refspec explícito desde una rama de feature |
| `git push origin main:feat/x` o `refs/remotes/origin/main:x` | Se evalúan los dos lados del refspec | Empujar desde la rama local |
| Herramienta PowerShell: `git log main; git push origin feat/x` | Su sintaxis no se analiza → regla anterior | Herramienta Bash |
| Una lectura de `docs/memory/` encadenada con cualquier orden que no sea de lectura (`ls docs/memory && npm test`, `… \| xargs …`) | R11 es una lista blanca | Separar en dos órdenes |
| Un heredoc que nombra `docs/memory` y no es `cat` solo (`cat <<'EOF' \| sh`, `bash <<'EOF'`, `<<EOF` sin comillas), o cualquier heredoc en PowerShell | Su cuerpo puede ejecutarse | `cat > f <<'EOF'` y luego la orden |
| `git add docs/memory/… && git commit …` | `git commit` no está en la lista de lecturas de R11 | Dos órdenes separadas |

## Criterios de aceptación

- **CA1** — Dada la orden exacta del caso 1 en una rama de feature, cuando pasa por `guard-shell.mjs` (Bash), entonces sale con 0. Con `git push origin feat/003-main-ui` y con `git diff origin/main && git push -u origin fix/002-x`, también.
- **CA2** — Dadas las dos órdenes del caso 2 (el heredoc `cat > … <<'EOF'` con `gh pr create --body-file`, y la lista con `node scripts/memory.mjs check`), cuando pasan por `protect-files.mjs` (Bash, orchestrator), entonces sale con 0.
- **CA3** — Dada la orden del caso 3, cuando pasa por `protect-files.mjs`, entonces sale con 0.
- **CA4** — Dada la orden del caso 4, también con `2>/dev/null`, cuando pasa por `protect-files.mjs`, entonces sale con 0.
- **CA5** — Dada cualquiera de las 21 formas de push a `main`/`master` del test `push_a_main_bloqueado`, cuando pasa por `checkPush`, entonces se niega con un motivo que contiene `main/master`.
- **CA6** — Dado cualquiera de los 9 push forzados de `push_forzado_bloqueado`, cuando pasa por `checkPush`, entonces se niega con un motivo que contiene `forzado`.
- **CA7** — Dado un push implícito (`git push`, `git push origin`, `git push -u origin HEAD`, `git push origin @`, `git push --tags`), cuando la rama actual es `main`, `master`, `""` o `null`, entonces se niega. En `fix/002-falsos-positivos-hooks`, se permite. Con un refspec explícito no se consulta la rama.
- **CA8** — Dada una escritura en `docs/memory/` fuera de la forma exacta, cuando pasa por `checkMemoryToolUse`, entonces se niega. Esto incluye todos los tests de `memory-policy*.test.mjs` existentes (sin modificar, salvo los tres de `memoryCommandLine` que T1 elimina) y los nuevos `politica_compuesto_con_escritura`, `politica_heredoc_cat_hacia_memoria` y `politica_heredoc_no_datos`.
- **CA9** — Dada una orden que el analizador no admite, o la herramienta PowerShell, cuando pasa por R5, entonces se aplica `legacyPushVerdict` (`push_no_analizable_bloqueado`, `push_powershell_regla_anterior`).
- **CA10** — Dada una excepción al analizar en `protect-files.mjs` o en `guard-shell.mjs`, cuando se ejecuta el hook, entonces sale con 2 y no con 1 (`hook_excepcion_bloquea`, `politica_orden_constructor`).
- **CA11** — Dado el plan aplicado, cuando un agente usa `Write` sobre `.claude/hooks/x.mjs` o ejecuta `echo x > docs/GOVERNANCE.md`, entonces `protect-files.mjs` sale con 0. Además, `rg -n "R8|PROTECTED_PATHS|isProtected|memoryCommandLine|ficheros de governance" -g '!docs/plans/00[01]-*' -g '!docs/plans/002-*'` solo devuelve el encabezado "R8 — Retirada" de `docs/GOVERNANCE.md`.
- **CA12** — Dado el plan aplicado, cuando se ejecuta `node scripts/quality-gate.mjs`, entonces termina en verde.

## Tareas

> Todas las tareas de código son **(developer)**, con TDD: primero los tests de la tabla (rojo) y después el código (verde). **(O)** = orchestrator. Los bloques de código de los tests son normativos (nombres, entradas y resultados). La implementación sigue el apartado Diseño.

### T1 — Retirar R8 (developer)
- **Ficheros/módulos:** `.claude/hooks/lib.mjs`, `.claude/hooks/protect-files.mjs`, `scripts/lib/memory-policy.mjs`, `scripts/lib/memory-policy.test.mjs`, `scripts/lib/memory-policy-evasion.test.mjs`, `docs/GOVERNANCE.md`, `.claude/agents/developer.md`, `scripts/hooks-regression.test.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** CA4, CA10, CA11
- **Subtareas:**
  - [x] T1.1 Crear `scripts/hooks-regression.test.mjs` con el arnés e2e:
    - `runHook(hook, payload)` → `spawnSync(process.execPath, [<ruta del hook>], { input: JSON.stringify(payload), encoding: "utf8", env: { ...process.env, CLAUDE_PROJECT_DIR: root, GIT_CEILING_DIRECTORIES: dirname(root) } })`.
    - `root` se crea con `mkdtempSync` y se borra en `finally`.
    - Ruta de los hooks con `fileURLToPath(import.meta.url)` + `join` (R3).
    - Los payloads llevan `tool_name`, `cwd: root` y `tool_input`.
  - [x] T1.2 `lib.mjs`: eliminar `PROTECTED_PATHS`, `toProjectRelative` e `isProtected`.
  - [x] T1.3 `protect-files.mjs`: dejar solo R11. Comentario de cabecera: "impide escribir la memoria fuera de scripts/memory.mjs (R11)". Import dinámico de `memory-policy.mjs`; si falta, se bloquea toda orden o fichero que mencione `docs/memory` (reserva fail-closed). `checkMemoryToolUse` va envuelto en `safely(check)`: una excepción → `{ allow: false, reason: "no se pudo analizar la orden (<mensaje>); ante la duda, se bloquea." }`. El matcher `Edit|Write|NotebookEdit` de `settings.json` se mantiene, porque R11 lo necesita.
  - [x] T1.4 `memory-policy.mjs`: eliminar `memoryCommandLine` y su comentario. En los tests, eliminar `linea_comando_memoria`, `linea_comando_ajeno` y `linea_comando_ordenes_tras_eof`, y su import. `COMMAND_AFTER_EOF` se queda, porque lo usa `politica_heredoc_con_ordenes_tras_eof`.
  - [x] T1.5 `docs/GOVERNANCE.md`:
    - §1, fila "Hooks de Claude Code": quitar "editar ficheros de governance, ".
    - §3: sustituir la sección R8 por `### R8 — Retirada` + "Retirada en el plan 002 por decisión del usuario: los agentes pueden modificar los ficheros de governance. El número se conserva para no renumerar R9–R11, a los que remiten los planes cerrados." No se renumera.
  - [x] T1.6 `developer.md` L22: "No editas los planes. Tampoco `docs/memory/`, salvo vía `node scripts/memory.mjs` (R11)."
  - [x] T1.7 Ejecutar el `rg` de CA11 y comprobar que solo devuelve el encabezado de R8.
- **Tests unitarios** (en `scripts/hooks-regression.test.mjs`):
  | Test | Hook · payload | Resultado esperado |
  |---|---|---|
  | `hook_write_en_hooks_permitido` | protect-files · `Write` `file_path: <root>/.claude/hooks/x.mjs` | 0 |
  | `hook_redireccion_a_governance_permitida` | protect-files · Bash · `echo x > docs/GOVERNANCE.md && sed -i s/a/b/ scripts/lib/x.mjs` | 0 |
  | `hook_caso_4_lectura` | protect-files · Bash · `ls docs/plans/ .claude/hooks/; git ls-tree --name-only origin/main .claude/hooks/; git fetch -q && git log --oneline -1 origin/main`, también con `2>/dev/null` | 0 |
  | `hook_write_memoria_sigue_bloqueado` | protect-files · `Write` `file_path: <root>/docs/memory/short/x.md` | 2, stderr `/R11/` |
  | `hook_r11_rm` | protect-files · Bash · `rm -rf docs/memory/short` | 2, stderr `/R11/` |
  | `hook_r11_sesion_permitida` | protect-files · `agent_type: developer`, `agent_id: a1` · `node scripts/memory.mjs session --agent developer --agent-id a1 --task T1 <<'EOF'\nresumen: x\nEOF` | 0 |
  | `hook_excepcion_bloquea` | protect-files · Bash · orchestrator · `constructor docs/memory` (hoy lanza `TypeError` → salida 1) | 2 |

### T2 — Analizador léxico de Bash (developer)
- **Ficheros/módulos:** `scripts/lib/shell-words.mjs` (nuevo), `scripts/lib/shell-words.test.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** base de CA1, CA2, CA5-CA9
- **Subtareas:**
  - [x] T2.1 Tests (rojo).
  - [x] T2.2 `export function parseCommandList(command)` → `Array<{ words: string[], redirects: { op: string, target: string }[] }> | null`, según el Diseño (1). Las órdenes vacías se descartan.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `palabras_orden_simple` | `git push -u origin feat/x` | `[["git","push","-u","origin","feat/x"]]` |
  | `palabras_separadores` | `a 1 && b; c \|\| d \| e & f\ng` | 7 órdenes |
  | `palabras_comillas` | `echo 'a; b' "c \| d" e"f"g` | `[["echo","a; b","c \| d","efg"]]` |
  | `palabras_escape_bash` | `git push origin ma\in` | `main` |
  | `palabras_continuacion_de_linea` | `ls \` + salto + `-la` | `[["ls","-la"]]` |
  | `palabras_placeholder_find` | `find . -exec rm {} \;` | `[..., "{}", ";"]` |
  | `palabras_redirecciones` | `cat a > b 2>&1 >> c &> d 2> e < f` | 6 redirecciones exactas |
  | `palabras_vacia` | `""`, `a;;b` | `[]`, `[["a"],["b"]]` |
  | `palabras_no_admitidas` | 15 entradas (código) | `null` |

```js
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
```

### T3 — Heredocs de datos (developer)
- **Ficheros/módulos:** `scripts/lib/shell-heredoc.mjs` (nuevo), `scripts/lib/shell-heredoc.test.mjs` (nuevo)
- **Depende de:** T2
- **Cubre:** CA2, CA8
- **Subtareas:**
  - [x] T3.1 Tests (rojo).
  - [x] T3.2 `export function stripDataHeredocs(command)` → `string`, según el Diseño (2). La línea de cabecera se conserva con el operador `<<'TAG'` sustituido por un espacio, y se eliminan las líneas del cuerpo y el terminador.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `heredoc_cat_se_despoja` | `cat > /tmp/pr.md <<'EOF'` + cuerpo + `EOF` + `gh pr create …` | `cat > /tmp/pr.md  \ngh pr create --body-file /tmp/pr.md` |
  | `heredoc_comillas_dobles_y_guion` | `cat <<-"FIN" > x\n\tdato\n\tFIN\nls` | `cat   > x\nls` |
  | `heredoc_varios_cat` | dos heredocs `cat` | se quitan los dos cuerpos |
  | `heredoc_sin_heredoc_igual` | `ls -la` | igual |
  | `heredoc_no_datos_intacto` | 10 entradas (código) | la orden sin cambios |

```js
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
```

### T4 — R5: push analizado (developer)
- **Ficheros/módulos:** `scripts/lib/push-guard.mjs` (nuevo), `scripts/lib/push-guard.test.mjs` (nuevo), `.claude/hooks/guard-shell.mjs`, `scripts/hooks-regression.test.mjs`
- **Depende de:** T1 (arnés e2e), T2, T3
- **Cubre:** CA1, CA5, CA6, CA7, CA9, CA10
- **Subtareas:**
  - [ ] T4.1 Tests unitarios y e2e (rojo).
  - [ ] T4.2 `push-guard.mjs`: `export function checkPush(command, { tool, currentBranch })` y `export function legacyPushVerdict(command)`, que devuelven `{ allow: true }` o `{ allow: false, reason }`. Motivos: `"prohibido el push forzado. R5."`, `"prohibido hacer push directo a main/master; usa una rama y PR. R5."` y `"no se puede analizar el push con certeza (variables, sustituciones o -c); escribe la orden con la rama literal. R5."`. `legacyPushVerdict` quita las `\` y aplica `\bgit\b[^;&|\n]*\bpush\b`, después la regex de fuerza anterior, después `/\b(main|master)\b/i` y, por último, opaco si el tramo desde `git … push` hasta el siguiente `;`, `&`, `|` o salto de línea contiene `$` o comillas invertidas.
  - [ ] T4.3 `guard-shell.mjs`: sustituir el bloque `if (isGit("push")) {…}` por `checkPush` con `tool: input.tool_name` y `currentBranch: () => git(["branch", "--show-current"])`. Import dinámico de `push-guard.mjs` con reserva a la regla anterior si falta, y `try/catch` que bloquea si hay una excepción. El resto del hook no cambia.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `push_caso_1_rama_de_feature` | orden exacta del caso 1 | permitido |
  | `push_main_fuera_del_push` | 3 listas con `main`/`master` fuera del push | permitido |
  | `push_rama_con_main_en_el_nombre` | `feat/003-main-ui`, `feat/004-domain` | permitido |
  | `push_sin_push` | `git log … main..HEAD`, `echo main --force`, `git stash push -m wip` en `main` | permitido |
  | `push_a_main_bloqueado` | 21 variantes | `main/master` |
  | `push_forzado_bloqueado` | 9 variantes | `forzado` |
  | `push_implicito_en_main_bloqueado` | 5 órdenes × ramas `main`, `master`, `""`, `null` | `main/master` |
  | `push_implicito_en_rama_permitido` | `git push`, `git push -u origin HEAD` en la rama de feature | permitido |
  | `push_explicito_no_consulta_la_rama` | `currentBranch` que lanza | permitido, sin lanzar |
  | `push_no_analizable_bloqueado` | `$BRANCH`, `"$(…)"`, comillas invertidas | `no se puede analizar` |
  | `push_configuracion_en_linea_bloqueada` | `git -c remote.origin.push=… push`, `git -c alias.p=push p …` | `no se puede analizar` |
  | `push_heredoc_de_datos` | heredoc `cat` con `git push --force origin main` en el cuerpo | permitido |
  | `push_powershell_regla_anterior` | PowerShell: `git push origin feat/x` / `git log main; git push origin feat/x` / `git push origin $branch` | permitido / `main/master` / `no se puede analizar` |
  | `hook_caso_1_push_a_rama` (e2e) | guard-shell · orden exacta del caso 1 | 0 |
  | `hook_push_main` (e2e) | guard-shell · `git push origin HEAD:main` | 2, `/main\/master/` |
  | `hook_push_forzado` (e2e) | guard-shell · `git push -uf origin feat/x` | 2, `/forzado/` |
  | `hook_push_implicito_sin_repo` (e2e) | guard-shell · `git push` (root no es repo: `git branch` falla) | 2 |
  | `hook_push_powershell` (e2e) | guard-shell · PowerShell · `git log main; git push origin feat/x` | 2 |

```js
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkPush } from "./push-guard.mjs";

const FEATURE = "fix/002-falsos-positivos-hooks";
const TO_MAIN = /main\/master/;
const FORCE = /forzado/;
const OPAQUE = /no se puede analizar/;

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
```

### T5 — R11: lecturas encadenadas y heredocs de datos (developer)
- **Ficheros/módulos:** `scripts/lib/memory-shell-syntax.mjs` (`splitCommandList`), `scripts/lib/memory-cli-form.mjs` (`export const READ_ONLY`), `scripts/lib/memory-readonly.mjs`, `scripts/lib/memory-policy.mjs` (`checkShellCommand`), `scripts/lib/memory-policy-compound.test.mjs` (nuevo), `scripts/hooks-regression.test.mjs`
- **Depende de:** T1, T3
- **Cubre:** CA2, CA3, CA8, CA10
- **Subtareas:**
  - [ ] T5.1 Tests unitarios y e2e (rojo).
  - [ ] T5.2 `splitCommandList(command)` → `string[] | null`. Parte por `&&`, `||`, `;` y `\n` fuera de comillas; un `&` suelto o un `|` se quedan en el elemento. Con comillas escapadas fuera de comillas o sin cerrar, `null`. Recorta cada elemento.
  - [ ] T5.3 `memory-readonly.mjs`:
    - `isAllowedRead` = todos los elementos no vacíos de `splitCommandList` (al menos uno) pasan `isAllowedItem`.
    - `isAllowedItem` quita `/(^|\s)[12]?>(&[12]|\/dev\/null)(?=\s|$)/g` y exige que todos los segmentos de `splitPipeline` sean lecturas.
    - `isAllowedSegment` añade `cd` (2 palabras como máximo), `pwd` (1) y `node` → `isAllowedNodeTest || READ_ONLY.includes(parseInvocation(segment)?.subcommand)`, y usa `Object.hasOwn(FORBIDDEN_OPTIONS, command)`.
  - [ ] T5.4 `memory-policy.mjs` `checkShellCommand`: si no es una invocación exacta, `effective = tool_name === "Bash" ? stripDataHeredocs(command) : command`, y se evalúa `effective`.
  - [ ] T5.5 Los tests existentes de `memory-policy*.test.mjs` siguen en verde sin cambios (salvo los tres eliminados en T1).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `lista_separadores` | `a; b && c \|\| d\ne \| f & g` | `["a","b","c","d","e \| f & g"]` |
  | `lista_comillas` | `grep "a; b" x && echo 'c \|\| d'` | 2 elementos |
  | `lista_no_admitida` | `echo "a`, `echo \'a` | `null` |
  | `politica_caso_2_heredoc_a_scratchpad` | caso 2a (orchestrator y developer) | permitido |
  | `politica_caso_2_compuesto_con_check` | caso 2b | permitido |
  | `politica_caso_3_compuesto_lectura` | caso 3 | permitido |
  | `politica_compuesto_redirecciones_inocuas` | `… 2>/dev/null; … 2>&1 \| head -5` | permitido |
  | `politica_cd_y_pwd` | `cd docs/memory && ls && pwd`, `ls docs/memory;` | permitido |
  | `politica_compuesto_con_escritura` | 8 órdenes (orchestrator y reviewer) | negado |
  | `politica_heredoc_cat_hacia_memoria` | `cat > docs/memory/long/errores.md <<'EOF'…` | negado, "solo se escribe" |
  | `politica_heredoc_no_datos` | 4 heredocs que no son datos | negado |
  | `politica_heredoc_powershell_sin_despojar` | caso 2a con PowerShell | negado |
  | `politica_orden_constructor` | `constructor docs/memory`, `toString docs/memory` | negado, sin excepción |
  | `hook_caso_2_heredoc_pr` (e2e) | protect-files · caso 2a con `<root>/pr.md` | 0 |
  | `hook_caso_2_compuesto_check` (e2e) | protect-files · caso 2b | 0 |
  | `hook_caso_3_lectura` (e2e) | protect-files · caso 3 | 0 |
  | `hook_r11_heredoc_a_memoria` (e2e) | protect-files · `cat > docs/memory/long/errores.md <<'EOF'\nx\nEOF` | 2, `/R11/` |
  | `hook_r11_lectura_y_escritura` (e2e) | protect-files · `ls docs/memory; rm x` | 2 |

```js
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
```

### T6 — Verificación en vivo de los hooks (O)
- **Depende de:** T1-T5 integradas (no hace falta reiniciar: `settings.json` no cambia)
- **Cubre:** CA1-CA11 en la sesión real
- **Subtareas:**
  - [ ] T6.1 Ejecutar en vivo con Bash las órdenes de **solo lectura** de los casos 2b, 3 y 4. Esperado: sin bloqueo.
  - [ ] T6.2 Caso 2a en vivo: el heredoc `cat > <scratchpad>/pr.md <<'EOF' … EOF` seguido de `ls <scratchpad>/pr.md` (sin `gh`). Esperado: sin bloqueo.
  - [ ] T6.3 Caso 1 por payload, sin hacer push real: escribir `<scratchpad>/p1.json` con `Write` y ejecutar `node .claude/hooks/guard-shell.mjs < <scratchpad>/p1.json` → 0. Con `git push origin main` → 2, y con `git push --force origin x` → 2.
  - [ ] T6.4 Sonda que debe bloquearse: `touch docs/memory/short/zz-sonda.md` → bloqueada con R11. Si pasara, se borra a mano.
  - [ ] T6.5 Anotar los resultados en este plan.
- **Tests unitarios:** no aplica (los automatizados son T1-T5).

### T7 — Revisión final, cierre de memoria y del plan (O + reviewer)
- **Depende de:** T1-T6
- **Subtareas:**
  - [ ] T7.1 El orchestrator lanza el `reviewer` con el plan, el diff completo, los resultados de T6 y `última tarea: sí`.
  - [ ] T7.2 El reviewer ejecuta `session` y `flush --task T7` y, con `APPROVED`, `close`. Como máximo, 3 ciclos.
  - [ ] T7.3 `node scripts/memory.mjs check` y `node scripts/quality-gate.mjs` en verde → `Estado: hecho`.

## Fuera de alcance

- Analizar la sintaxis de PowerShell: esa herramienta conserva la regla anterior de R5 y la lista blanca de R11 sin despojar heredocs.
- Los demás falsos positivos de `guard-shell.mjs` que miran toda la orden: `--no-verify`, `\s-n\b` con `git commit`, las líneas de atribución y `core.hooksPath`. Van en un plan aparte.
- Alias de git definidos en la configuración (`git p` → `push`), `remote.<x>.push` y `remote.pushDefault` ya configurados.
- Sustituir R8 por otro mecanismo de protección (por ejemplo, CODEOWNERS): la autoridad final sigue siendo la CI con la protección de rama.
- Planes cerrados 000 y 001: no se editan aunque mencionen R8.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Sin R8, un agente puede debilitar los hooks, el gate o sus tests | Decisión explícita del usuario. El reviewer revisa todo diff a `.claude/`, `scripts/` y `.github/`. La CI y la protección de rama siguen siendo la autoridad final (GOVERNANCE §1). Si se quiere recuperar, se hará en un plan nuevo. |
| Un fallo del analizador abre un hueco en R5 o R11 | Con `null` se aplica la regla anterior (R5) o la lista blanca sin relajar (R11). Los tests de evasión cubren escapes, comillas, `-C`, `env`, `-c`, alias, comodines, mayúsculas, abreviaturas y heredocs maliciosos (comilla abierta en la línea anterior, `cat >'<<'EOF''`, tuberías e intérpretes). |
| Un hook lanza una excepción y sale con 1, que es no bloqueante (fail-open) | `safely` en `protect-files.mjs`, `try/catch` en `guard-shell.mjs` y `Object.hasOwn`. Tests `hook_excepcion_bloquea` y `politica_orden_constructor`. |
| Falsos positivos nuevos por ser fail-closed (push implícito con `git` caído, `$BRANCH`) | Tabla de casos ambiguos con su alternativa. |
| `git branch --show-current` falla o tarda | Solo se consulta en un push implícito. Si falla (`null`), se bloquea. |
| El e2e depende del binario `git` (`hook_push_implicito_sin_repo`) | `GIT_CEILING_DIRECTORIES` evita que encuentre un repo padre. La CI tiene `git`. Sin red. |
| Al eliminar `memoryCommandLine` se cambia el comportamiento de R11 | Solo la usaba el bloque R8. `checkShellCommand` usa `parseInvocation`, que no cambia. Los tests de heredoc tras `EOF` (`politica_heredoc_con_ordenes_tras_eof`) se mantienen. |

## Preguntas abiertas

- Ninguna bloqueante. La causa exacta del caso 4 (probablemente una redirección omitida en el resumen) ya no importa: el bloqueo era de R8, que se retira, y `hook_caso_4_lectura` comprueba la orden con y sin `2>/dev/null`.
