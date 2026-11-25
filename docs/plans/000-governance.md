# 000 — Governance inicial y reglas R9 (mensajes de commit) y R10 (rama dedicada)

- **Tipo:** chore
- **Estado:** hecho
- **Rama:** chore/000-governance
- **Fase del roadmap:** previa a la fase 0 (Esqueleto) de [docs/MVP.md](../MVP.md)

## Objetivo

Dejar el repositorio con su primer commit, que contiene toda la governance, y con dos reglas nuevas aplicadas por máquina: mensajes de commit legibles y uniformes sin líneas de atribución (R9), y cada feature en su propia rama ligada a un plan (R10). Así cualquier historial futuro se puede leer y auditar sin depender de la disciplina de nadie.

## Contexto

- El repo local **no tiene ningún commit**. La rama actual `chore/000-governance` está sin nacer (unborn) y `main` aún no existe.
- El remoto `devlitus/app-llama` ya existe en GitHub, pero está vacío y sin rama por defecto.
- Todo lo que hay en el árbol de trabajo es el setup de governance y debe entrar en el primer commit:
  - `.claude/`: agentes, con `model:` y `color:` en el frontmatter; hooks; `settings.json`.
  - `.githooks/pre-commit`.
  - `.github/`: CI, plantilla de issue y plantilla de PR.
  - `.gitignore`.
  - `CLAUDE.md`, con la columna Modelo en la tabla del equipo y los pasos 4 y 8 del flujo ya actualizados.
  - `docs/`: `GOVERNANCE.md`, `MVP.md`, `plans/_TEMPLATE.md` y este plan.
  - `scripts/quality-gate.mjs`.
- **R8:** `.githooks/**`, `.claude/hooks/**`, `.claude/settings.json`, `.github/workflows/**`, `docs/GOVERNANCE.md` y `scripts/quality-gate.mjs` son ficheros protegidos. Los modifica **el usuario a mano**. Ningún agente los toca. En este plan no interviene el developer.
- Borradores listos para aplicar, en `$DRAFT` = `C:\Users\carle\AppData\Local\Temp\claude\622cc37fcedd5642\5add4e87-2e05-4d92-89da-d75a662c864c\scratchpad\governance`. Ambos hooks tienen ya finales de línea LF.
  - `githooks\commit-msg`: hook `sh` que ejecuta `node .githooks/commit-msg.mjs "$1"`.
  - `githooks\commit-msg.mjs`: validador R9. Admite `<fichero-del-mensaje>` (hook local) o `--range <base>..<head>` (CI). Funciona así:
    1. Quita las líneas que empiezan por `#`.
    2. Valida el asunto con `^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9][a-z0-9-]*\))?!?: \S.*$`. Comprueba además un máximo de 72 caracteres, que no acabe en punto y que haya una línea en blanco tras el asunto.
    3. Rechaza las líneas de atribución con estos patrones: `/^\s*[a-z]+(-[a-z]+)*-by\s*:/im` (Co-Authored-By, Signed-off-by, Reviewed-by…), `/generated (with|by)\b/i`, `/noreply@anthropic\.com/i` y `/\u{1F916}/u` (🤖). El motivo es `prohibidas las líneas de atribución (Co-Authored-By, Signed-off-by, Generated with…).`
    4. Exenciones: `Merge ` y `Revert "` siempre están exentos. `fixup! `, `squash! ` y `amend! ` se permiten en modo fichero, pero en modo `--range` se rechazan con `commit de autosquash sin integrar: "<asunto>". Ejecuta git rebase --autosquash antes del PR.`
  - `guard-shell.patch.md`: parches para `guard-shell.mjs` (R9: los mismos patrones de atribución sobre el comando `git commit`; R10: rama y plan), `GOVERNANCE.md` y `ci.yml`. Añade `PROJECT_DIR` al import existente de `./lib.mjs`, sin duplicarlo.
- Estado actual relevante:
  - `.claude/hooks/guard-shell.mjs` ya bloquea los commits en `main`/`master` y los commits sin `core.hooksPath = .githooks`. Detecta un commit con `isGit("commit")` sobre el texto del comando.
  - `.claude/hooks/lib.mjs` exporta `PROJECT_DIR`, `block` (sale con código 2) y `git`.
  - `.github/workflows/ci.yml` hace checkout sin `fetch-depth` y corre el quality gate en `windows-latest` y `macos-latest`.
  - `scripts/quality-gate.mjs`: sin `Cargo.toml` ni `ui/package.json`, solo ejecuta el escaneo de patrones prohibidos, y hoy está en verde.
- Ajuste GitHub: "Automatically delete head branches" (Settings → General → Pull Requests) corresponde al campo `delete_branch_on_merge` de la API REST de repos.

## Decisiones

- **D1 (antes P1), base de `main`:** el usuario crea a mano en `main` un commit raíz vacío (T6). La rama `chore/000-governance` nace de él y la governance entra por PR con CI.
- **D2 (antes P2), remoto:** el repo es `devlitus/app-llama`, vacío y sin rama por defecto. `main` se sube primero para que sea la rama por defecto (T12).
- **D3 (antes P3), atribución:** se rechazan **todas** las líneas de atribución con los patrones de "Contexto", tanto en el validador como en guard-shell. Mencionar "Claude" en un mensaje (por ejemplo `docs: actualiza CLAUDE.md`) no se bloquea.
- **D4 (antes P5), autosquash:** `fixup!`, `squash!` y `amend!` se permiten en el hook local y se rechazan en CI (`--range`). Los merges y `Revert "` siguen exentos.
- **D5 (antes P6), bots:** no se usarán Dependabot ni Renovate. R10 no tiene excepciones.

## Criterios de aceptación

- **CA1 (R9, mensaje válido):** Dado el hook `commit-msg` instalado, cuando se valida en modo fichero un mensaje de la lista de válidos de T7, entonces `node .githooks/commit-msg.mjs <fichero>` sale con código 0 y stderr queda vacío. La lista incluye `docs: actualiza CLAUDE.md` y `fixup! feat: a`.
- **CA2 (R9, mensaje inválido):** Dado el mismo validador, cuando se valida cada mensaje de la lista de inválidos de T7, entonces:
  - sale con código 1;
  - stderr contiene una línea `[R9] mensaje: <motivo>` con el motivo de la tabla;
  - stderr termina con la línea `Commit rechazado. Ver docs/GOVERNANCE.md (R9).`
  
  Los mensajes con `Co-Authored-By`, `Signed-off-by`, `Generated with Claude Code`, `noreply@anthropic.com` o 🤖 dan el motivo `prohibidas las líneas de atribución`.
- **CA3 (R9, git real):** Dado `core.hooksPath = .githooks` en un repo de prueba con los hooks copiados, cuando se ejecuta `git commit --allow-empty -F <fichero>` con un mensaje inválido, entonces git rechaza el commit: código distinto de 0 y `git rev-list --all --count` sin cambios. Con un mensaje válido el commit se crea. Con `git commit -s` (que añade `Signed-off-by`) el commit se rechaza.
- **CA4 (R9, rango para CI):** Dado un repo de prueba con commits válidos y un merge `Merge branch 'main' into feat/001-x`, cuando se ejecuta `node .githooks/commit-msg.mjs --range <base>..<head>`, entonces sale con código 0.
  - Si se añade un commit inválido o un `fixup! feat: a`, sale con código 1 y reporta **solo** ese commit con su SHA corto (8 caracteres).
  - Para el fixup, el motivo es `Ejecuta git rebase --autosquash antes del PR`.
- **CA5 (R9, guard-shell):** Dado `guard-shell.mjs` parcheado, cuando recibe por stdin un comando `git commit` que contiene `Co-Authored-By:` (en cualquier combinación de mayúsculas), `Signed-off-by:`, `Generated with Claude Code`, `noreply@anthropic.com` o 🤖, entonces sale con código 2 y stderr contiene `prohibidas las líneas de atribución` y `R9`. Con `git commit -m "docs: actualiza CLAUDE.md"`, en una rama válida con plan, sale con código 0.
- **CA6 (R10, guard-shell):** Dado un repo de prueba con `core.hooksPath = .githooks` y `CLAUDE_PROJECT_DIR` apuntando a él, cuando guard-shell recibe `{"tool_input":{"command":"git commit -m \"chore: x\""}}`, entonces:
  - con cada rama válida de T8 cuyo plan existe, sale con código 0;
  - con cada rama inválida de T8, sale con código 2 y el motivo `no sigue feat|fix|chore/NNN-slug ... R10`;
  - con una rama válida sin plan, sale con código 2 y el motivo `no existe docs/plans/<slug>.md ... R10`.
- **CA7 (CI):** Dado `ci.yml` parcheado, entonces:
  - `actions/checkout@v4` tiene `fetch-depth: 0`;
  - existen los pasos `Mensajes de commit (R9)` y `Rama dedicada (R10)`, ambos con `if: github.event_name == 'pull_request'` y `shell: bash`;
  - el paso R10 lee la rama desde `env.BRANCH: ${{ github.head_ref }}`, nunca interpolada en `run`;
  - comprobación local: ejecutado en bash con cada rama de T8, el bloque `run` del paso R10 acepta y rechaza las mismas ramas que guard-shell.
- **CA8 (GOVERNANCE):** Dado `docs/GOVERNANCE.md` parcheado, entonces:
  - la tabla §1 incluye `Git hooks pre-commit y commit-msg`;
  - existen las secciones `### R9 — Mensajes de commit`, con la línea `Prohibida cualquier línea de atribución` y la de `fixup!`/`squash!`/`amend!`, y `### R10 — Una rama dedicada por feature`, ambas tras R8;
  - R5 dice `→ rama dedicada (R10) →`;
  - la última línea de §4 es `- [ ] Commit con Conventional Commits y sin Co-Authored-By (R9).`
- **CA9 (ejecutables):** Dado el índice de git, cuando se ejecuta `git ls-files -s .githooks`, entonces `.githooks/pre-commit` y `.githooks/commit-msg` aparecen con modo `100755`.
- **CA10 (commit inicial):** Dado todo lo anterior, cuando el orchestrator hace el commit inicial sobre el commit raíz de `main`, entonces:
  - `git log --format=%B -1` cumple R9 y no tiene líneas de atribución;
  - el commit está en `chore/000-governance` y su padre es `main`;
  - `git show --stat HEAD` incluye todos los ficheros de T10;
  - `git status --porcelain` queda vacío;
  - se han ejecutado pre-commit y commit-msg (sin `--no-verify`).
- **CA11 (GitHub):** Dado el push de T12, entonces:
  - `gh api repos/devlitus/app-llama --jq .default_branch` devuelve `main`;
  - `gh api repos/devlitus/app-llama --jq .delete_branch_on_merge` devuelve `true`;
  - en el PR `chore/000-governance` → `main`, los pasos R9 y R10 están en verde en los dos SO.

## Tareas

> Quién: **U** = usuario a mano (ficheros protegidos por R8, refs en `main` o ajustes de GitHub). **O** = orchestrator. En este plan no interviene el developer.

### T1 — Instalar el hook commit-msg (U)
- **Ficheros/módulos:** `.githooks/commit-msg`, `.githooks/commit-msg.mjs`
- **Depende de:** —
- **Cubre:** CA1, CA2, CA3, CA4
- **Subtareas:**
  - [x] T1.1 Copiar `$DRAFT\githooks\commit-msg` a `.githooks/commit-msg` y `$DRAFT\githooks\commit-msg.mjs` a `.githooks/commit-msg.mjs`, sin reconvertir los finales de línea.
  - [x] T1.2 Verificar con `git config core.hooksPath` que el valor es `.githooks`. Si no lo es, ejecutar `git config core.hooksPath .githooks`.
- **Tests unitarios:** ver T7.

### T2 — Parchear guard-shell para R9 y R10 (U)
- **Ficheros/módulos:** `.claude/hooks/guard-shell.mjs`
- **Depende de:** —
- **Cubre:** CA5, CA6
- **Subtareas:**
  - [x] T2.1 Al principio del bloque `if (isGit("commit")) { ... }`, añadir el bloqueo de atribución del borrador: `/[a-z]+(-[a-z]+)*-by\s*:|generated (with|by)\b|noreply@anthropic\.com|\u{1F916}/iu` sobre `command`.
  - [x] T2.2 Sustituir el import de `./lib.mjs` por `import { readHookInput, block, git, PROJECT_DIR } from "./lib.mjs";`. Añadir los imports de `node:fs` y `node:path`, la constante `BRANCH` y la función `planFor(branch)` del borrador.
  - [x] T2.3 Tras el bloqueo de `main`/`master`, añadir los dos bloqueos R10: primero el formato de la rama y después la existencia del plan. Con ese orden, una rama `null` (HEAD separado) se rechaza por formato antes de llamar a `planFor`.
- **Tests unitarios:** ver T8.

### T3 — Parchear la CI (U)
- **Ficheros/módulos:** `.github/workflows/ci.yml`
- **Depende de:** T1 (el paso R9 invoca `.githooks/commit-msg.mjs`)
- **Cubre:** CA7
- **Subtareas:**
  - [x] T3.1 Añadir `with: fetch-depth: 0` a `actions/checkout@v4`.
  - [x] T3.2 Tras `actions/setup-node@v4`, añadir el paso `Mensajes de commit (R9)` del borrador.
  - [x] T3.3 Añadir a continuación el paso `Rama dedicada (R10)` del borrador, con `BRANCH` pasado por `env`.
  - [x] T3.4 Revisar la indentación a ojo (2 espacios, pasos bajo `steps:`). La validación real ocurre en el PR de T12.
- **Tests unitarios:** ver T9.

### T4 — Parchear GOVERNANCE.md (U)
- **Ficheros/módulos:** `docs/GOVERNANCE.md`
- **Depende de:** —
- **Cubre:** CA8
- **Subtareas:**
  - [x] T4.1 Tabla §1: sustituir la fila del git hook por `Git hooks pre-commit y commit-msg`. En la fila de hooks de Claude Code, añadir los commits con líneas de atribución.
  - [x] T4.2 Añadir `### R9 — Mensajes de commit` tras R8 (texto del borrador, que ya incluye la atribución ampliada y el autosquash).
  - [x] T4.3 Añadir `### R10 — Una rama dedicada por feature` tras R9.
  - [x] T4.4 En R5, cambiar `→ rama →` por `→ rama dedicada (R10) →`.
  - [x] T4.5 En §4, cambiar la última línea por `- [ ] Commit con Conventional Commits y sin Co-Authored-By (R9).`
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `governance_tiene_r9` | `Select-String -Path docs/GOVERNANCE.md -Pattern '^### R9 — Mensajes de commit$'` | 1 coincidencia |
  | `governance_tiene_r10` | `Select-String ... -Pattern '^### R10 — Una rama dedicada por feature$'` | 1 coincidencia |
  | `governance_orden_r8_r9_r10` | Números de línea de `### R8`, `### R9` y `### R10` | Estrictamente crecientes |
  | `governance_r9_atribucion` | `Select-String ... -Pattern 'Prohibida cualquier línea de atribución'` | 1 coincidencia |
  | `governance_r9_autosquash` | `Select-String ... -Pattern 'rebase --autosquash'` | 1 coincidencia |
  | `governance_r5_rama_dedicada` | `Select-String ... -Pattern 'rama dedicada \(R10\)'` | ≥ 1 coincidencia dentro de R5 |
  | `governance_dod_r9` | Última línea con `- [ ]` de §4 | `- [ ] Commit con Conventional Commits y sin Co-Authored-By (R9).` |
  | `governance_tabla_commit_msg` | `Select-String ... -Pattern 'pre-commit. y .commit-msg'` | 1 coincidencia en §1 |

### T5 — Revisión de la governance antes del commit (O, lanza el `reviewer`)
- **Ficheros/módulos:** todo el árbol de trabajo (no hay commit base que sirva de diff)
- **Depende de:** T1, T2, T3, T4, T7, T8, T9
- **Cubre:** R5 (revisión `APPROVED` antes del commit)
- **Subtareas:**
  - [x] T5.1 Pasar al reviewer la ruta de este plan, los resultados de T7, T8 y T9, y la indicación de que no hay rango de diff: debe revisar los ficheros de T10 contra CA1–CA9.
  - [x] T5.2 El reviewer abre los issues en `devlitus/app-llama`.
  - [x] T5.3 Si el veredicto es `CHANGES_REQUESTED`, los cambios en ficheros protegidos los aplica **el usuario** (R8). Como máximo 3 ciclos.
- **Tests unitarios:** no aplica (es una revisión).

### T6 — Preparar `main` con un commit raíz vacío (U, en su terminal)
- **Ficheros/módulos:** ninguno (solo refs de git)
- **Depende de:** T1 (el commit raíz también pasa R9)
- **Cubre:** D1, y permite el PR de T12
- **Subtareas:** se hacen fuera de Claude Code, porque guard-shell bloquea con razón los commits en `main`.
  - [x] T6.1 `git symbolic-ref HEAD refs/heads/main`. El árbol de trabajo sigue intacto y sin seguimiento.
  - [x] T6.2 `git commit --allow-empty -m "chore: commit raíz del repositorio"`. Pasan pre-commit (gate en verde) y commit-msg.
  - [x] T6.3 `git switch -c chore/000-governance`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `main_tiene_raiz_vacia` | `git rev-list --count main` y `git show --stat --format= main` | `1` y salida vacía |
  | `rama_nace_de_main` | `git merge-base main chore/000-governance` | Igual a `git rev-parse main` |

### T7 — Verificar el validador R9 (U, en su terminal)
- **Ficheros/módulos:** `.githooks/commit-msg.mjs`, `.githooks/commit-msg`
- **Depende de:** T1
- **Cubre:** CA1, CA2, CA3, CA4
- **Método (determinista y sin red):**
  - Cada mensaje se escribe con LF en un fichero dentro de un directorio temporal nuevo (por ejemplo `$env:TEMP\r9-cases\<test>.txt`), con codificación UTF-8 sin BOM para los casos con acentos o 🤖.
  - Se ejecuta `node .githooks/commit-msg.mjs <fichero>` y se comprueban el código de salida y el stderr.
  - Se hace en la terminal del usuario y no desde Claude Code, porque guard-shell bloquearía los comandos que mezclan `git commit` y atribución.
  - `\n` indica salto de línea.
- **Tests unitarios (válidos en modo fichero, código 0, stderr vacío):**
  | Test | Entrada (contenido del fichero) | Resultado esperado |
  |---|---|---|
  | `r9_acepta_feat_con_ambito` | `feat(hf): búsqueda de modelos GGUF` | 0 |
  | `r9_acepta_sin_ambito` | `fix: corrige la reanudación de descargas` | 0 |
  | `r9_acepta_breaking` | `feat!: cambia el formato de perfiles` | 0 |
  | `r9_acepta_breaking_con_ambito` | `refactor(core)!: separa runtime de server` | 0 |
  | `r9_acepta_ambito_con_guion` | `build(src-tauri): fija la versión de tauri` | 0 |
  | `r9_acepta_todos_los_tipos` | Un mensaje `<tipo>: cambio de prueba` por cada tipo: `docs`, `style`, `perf`, `test`, `build`, `ci`, `chore`, `revert` | 0 en los 8 |
  | `r9_acepta_72_caracteres` | `chore: ` + 65 veces `a` (72 en total) | 0 |
  | `r9_acepta_cuerpo` | `feat(ui): añade la vista de biblioteca\n\nLista los modelos descargados.` | 0 |
  | `r9_acepta_mencion_claude` | `docs: actualiza CLAUDE.md` | 0 (mencionar Claude no es atribución) |
  | `r9_ignora_comentarios` | `chore: limpia scripts\n# Please enter the commit message\n# On branch chore/000-governance` | 0 |
  | `r9_acepta_crlf` | `fix: corrige rutas\r\n\r\nUsa PathBuf.` | 0 |
  | `r9_omite_merge` | `Merge branch 'main' into feat/001-hf-search` | 0 |
  | `r9_omite_revert_git` | `Revert "feat(hf): búsqueda"` | 0 |
  | `r9_local_permite_fixup` | `fixup! feat: a` | 0 |
  | `r9_local_permite_squash_amend` | `squash! feat: a` y `amend! feat: a` | 0 en los 2 |
- **Tests unitarios (inválidos en modo fichero, código 1):**
  | Test | Entrada | Motivo esperado en stderr |
  |---|---|---|
  | `r9_rechaza_sin_tipo` | `añade búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_tipo_desconocido` | `feature(hf): búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_tipo_mayusculas` | `Feat: búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_ambito_mayusculas` | `feat(HF): búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_ambito_vacio` | `feat(): búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_breaking_mal_colocado` | `feat!(hf): búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_sin_espacio` | `feat:búsqueda` | `no sigue Conventional Commits` |
  | `r9_rechaza_descripcion_vacia` | `feat: ` | `no sigue Conventional Commits` |
  | `r9_rechaza_mensaje_vacio` | Fichero vacío o solo con comentarios `#` | `no sigue Conventional Commits` |
  | `r9_rechaza_73_caracteres` | `chore: ` + 66 veces `a` (73 en total) | `tiene 73 caracteres (máximo 72)` |
  | `r9_rechaza_punto_final` | `fix: corrige la descarga.` | `no debe terminar en punto` |
  | `r9_rechaza_sin_linea_en_blanco` | `fix: corrige la descarga\nDetalle del cuerpo` | `deja una línea en blanco` |
  | `r9_rechaza_co_authored_by` | `feat: x\n\nCo-Authored-By: Claude <noreply@anthropic.com>` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_co_authored_by_minusculas` | `feat: x\n\nco-authored-by: alguien <a@b.c>` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_trailer_con_sangria` | `feat: x\n\n   Co-authored-by : a <a@b.c>` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_signed_off_by` | `feat: x\n\nSigned-off-by: Ana <ana@ejemplo.com>` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_reviewed_by` | `feat: x\n\nReviewed-by: Ana <ana@ejemplo.com>` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_generated_with` | `feat: x\n\nGenerated with Claude Code` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_generated_by` | `feat: x\n\nTexto generated by una herramienta` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_noreply_anthropic` | `feat: x\n\nContacto: noreply@anthropic.com` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_emoji_robot` | `feat: x\n\n🤖 hecho con ayuda` | `prohibidas las líneas de atribución` |
  | `r9_rechaza_emoji_robot_en_asunto` | `feat: 🤖 añade chat` | `prohibidas las líneas de atribución` |
  | `r9_acumula_errores` | `Fix: algo.` | 2 líneas `[R9]` (formato y punto final), código 1 |
- **Tests de integración con git (CA3):** repo temporal nuevo (`git init $env:TEMP\r9-repo`) con `.githooks/` y `scripts/quality-gate.mjs` copiados, y `git config core.hooksPath .githooks`. Los commits se hacen con `git -c user.name=t -c user.email=t@t`.
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `r9_hook_bloquea_commit` | `commit --allow-empty -F <r9_rechaza_punto_final>` | Código ≠ 0, `git rev-list --all --count` = 0 |
  | `r9_hook_permite_commit` | `commit --allow-empty -F <r9_acepta_sin_ambito>` | Código 0, 1 commit |
  | `r9_hook_rechaza_signoff` | `commit --allow-empty -s -m "fix: b"` | Código ≠ 0 (git añade `Signed-off-by` antes del hook), número de commits sin cambios |
- **Tests de rango (CA4):** en el mismo repo temporal, los commits del historial se crean con `git -c core.hooksPath=<directorio-vacío> commit --allow-empty -F <fichero>`, solo en este repo de prueba, para poder tener commits inválidos.
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `r9_rango_todo_valido` | base = commit 1. Commits: `feat: a`, `fix(core): b` y un merge `Merge branch 'main' into feat/001-x` | `--range base..HEAD` da 0 |
  | `r9_rango_detecta_invalido` | Lo anterior más `arregla cosas` | 1, una sola línea `[R9] <sha8>:`, con `<sha8>` = `git rev-parse --short=8` del commit inválido |
  | `r9_rango_rechaza_fixup` | Lo de `r9_rango_todo_valido` más `fixup! feat: a` | 1, una sola línea `[R9] <sha8>: commit de autosquash sin integrar: "fixup! feat: a". Ejecuta git rebase --autosquash antes del PR.` |
  | `r9_rango_rechaza_squash_amend` | Ídem con `squash! feat: a` y `amend! feat: a` | 1, dos líneas `[R9]`, una por SHA |
  | `r9_rango_omite_revert` | Ídem con `Revert "feat: a"` | 0 |
  | `r9_rango_detecta_atribucion` | Commit `feat: c\n\nCo-Authored-By: x <x@x>` en el rango | 1, `prohibidas las líneas de atribución` |
  | `r9_rango_vacio` | `--range HEAD..HEAD` | 0 |
  | `r9_rango_invalido` | `--range noexiste..HEAD` | 1, el error de git sale por stderr |

### T8 — Verificar guard-shell R9 y R10 (U, en su terminal)
- **Ficheros/módulos:** `.claude/hooks/guard-shell.mjs`
- **Depende de:** T2
- **Cubre:** CA5, CA6
- **Método (determinista y sin red):**
  - Crear un repo temporal nuevo (`git init $env:TEMP\r10-repo`) con `git config core.hooksPath .githooks` y `docs/plans/` con estos planes vacíos: `000-governance.md`, `001-hf-search.md`, `012-a.md` y `123-x1-y2.md`.
  - Para elegir la rama de cada caso: `git symbolic-ref HEAD refs/heads/<rama>`.
  - Para ejecutar cada caso: `$env:CLAUDE_PROJECT_DIR="$env:TEMP\r10-repo"; Get-Content -Encoding utf8 case.json | node W:\app-llama\.claude\hooks\guard-shell.mjs`. Por defecto, `case.json` contiene `{"tool_input":{"command":"git commit -m \"chore: x\""}}`.
  - El JSON va en un fichero para que el comando lanzado no contenga `git commit`.
  - Se comprueban `$LASTEXITCODE` y el stderr.
- **Tests unitarios (ramas válidas con plan, código 0):**
  | Test | Rama | Resultado esperado |
  |---|---|---|
  | `r10_acepta_chore` | `chore/000-governance` | 0 |
  | `r10_acepta_feat_multipalabra` | `feat/001-hf-search` | 0 |
  | `r10_acepta_fix_una_letra` | `fix/012-a` | 0 |
  | `r10_acepta_alfanumerico` | `feat/123-x1-y2` | 0 |
- **Tests unitarios (ramas inválidas, código 2, motivo `no sigue feat|fix|chore/NNN-slug`):**
  | Test | Rama | Por qué |
  |---|---|---|
  | `r10_rechaza_prefijo_feature` | `feature/001-hf-search` | Prefijo no admitido |
  | `r10_rechaza_prefijo_mayusculas` | `Feat/001-hf-search` | Prefijo en mayúsculas |
  | `r10_rechaza_dos_digitos` | `feat/01-hf` | NNN debe tener 3 dígitos |
  | `r10_rechaza_cuatro_digitos` | `feat/0001-hf` | NNN debe tener 3 dígitos |
  | `r10_rechaza_sin_slug` | `feat/001` | Falta el slug |
  | `r10_rechaza_slug_vacio` | `feat/001-` | Slug vacío |
  | `r10_rechaza_slug_mayusculas` | `feat/001-HF` | El slug va en minúsculas |
  | `r10_rechaza_guion_bajo` | `feat/001-hf_search` | Solo se admite `[a-z0-9-]` |
  | `r10_rechaza_doble_guion` | `feat/001-hf--search` | Segmento vacío |
  | `r10_rechaza_guion_final` | `feat/001-hf-` | Segmento final vacío |
  | `r10_rechaza_subruta` | `feat/001-hf/extra` | Solo se admite una `/` |
  | `r10_rechaza_sin_prefijo` | `001-hf-search` | Falta el tipo |
  | `r10_rechaza_head_separado` | HEAD separado: `git checkout --detach` tras un commit vacío hecho con `-c core.hooksPath=<directorio-vacío>` | `git branch --show-current` vacío; código 2 |
- **Tests unitarios (otros casos, rama `chore/000-governance` salvo que se indique otra):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `r10_rechaza_sin_plan` | Rama `feat/999-no-existe` | 2, `no existe docs/plans/999-no-existe.md` |
  | `r10_rechaza_plan_distinto` | Rama `feat/001-hf` (solo existe `001-hf-search.md`) | 2, `no existe docs/plans/001-hf.md` |
  | `r10_main_sigue_bloqueado` | Rama `main` | 2, `no se hacen commits en 'main'` (R5 se evalúa antes que R10) |
  | `r9_guard_bloquea_co_author` | `git commit -m "feat: x" -m "Co-Authored-By: A <a@b.c>"` | 2, `prohibidas las líneas de atribución` y `R9` |
  | `r9_guard_bloquea_co_author_minusculas` | Igual, con `co-authored-by: a <a@b.c>` | 2, el mismo motivo |
  | `r9_guard_bloquea_signed_off_by` | `git commit -m "feat: x" -m "Signed-off-by: Ana <ana@ejemplo.com>"` | 2, el mismo motivo |
  | `r9_guard_bloquea_generated_with` | `git commit -m "feat: x" -m "Generated with Claude Code"` | 2, el mismo motivo |
  | `r9_guard_bloquea_noreply` | `git commit -m "feat: x" -m "x <noreply@anthropic.com>"` | 2, el mismo motivo |
  | `r9_guard_bloquea_emoji_robot` | `git commit -m "feat: x" -m "🤖"` | 2, el mismo motivo |
  | `r9_guard_permite_mencion_claude` | `git commit -m "docs: actualiza CLAUDE.md"` | 0 |
  | `guard_ignora_no_commit` | Rama `feature/x` (inválida) con el comando `git status` | 0 (R10 solo se aplica a los commits) |
  | `guard_sin_hookspath` | `git config --unset core.hooksPath` | 2, `el git hook pre-commit no está activo` |

### T9 — Verificar los pasos de CI en local (U, en Git Bash)
- **Ficheros/módulos:** `.github/workflows/ci.yml`
- **Depende de:** T3
- **Cubre:** CA7
- **Método:** copiar el bloque `run` del paso `Rama dedicada (R10)` a `r10.sh` en un directorio temporal. Situarse en el repo temporal de T8 y ejecutar `BRANCH=<rama> bash r10.sh`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `ci_r10_paridad_validas` | Las 4 ramas válidas de T8 | Código 0 en todas |
  | `ci_r10_paridad_invalidas` | Las 12 ramas inválidas por formato de T8 (todas menos HEAD separado) | Código 1 y `R10: rama '<rama>' no válida` |
  | `ci_r10_rama_vacia` | `BRANCH=` | Código 1, `no válida` |
  | `ci_r10_sin_plan` | `BRANCH=feat/999-no-existe` | Código 1, `R10: falta docs/plans/999-no-existe.md` |
  | `ci_r10_no_inyecta` | `BRANCH='feat/001-x$(touch pwned)'` | Código 1 y no se crea el fichero `pwned` |
  | `ci_yaml_fetch_depth` | `Select-String .github/workflows/ci.yml -Pattern 'fetch-depth: 0'` | 1 coincidencia |
  | `ci_yaml_pasos_pr` | Pasos R9 y R10 en `ci.yml` | Los dos tienen `if: github.event_name == 'pull_request'` |

### T10 — Marcar los hooks como ejecutables y preparar el índice (O)
- **Ficheros/módulos:** índice de git
- **Depende de:** T5 (`APPROVED`), T6
- **Cubre:** CA9
- **Subtareas:**
  - [x] T10.1 Marcar este plan como `Estado: hecho` y las tareas T1–T10 como completadas.
  - [x] T10.2 `git add -A` y revisar con `git status --porcelain` que no entra nada ignorado ni ajeno al setup.
  - [x] T10.3 `git update-index --chmod=+x .githooks/pre-commit .githooks/commit-msg`. Va después del `git add`, porque solo funciona sobre rutas ya indexadas.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `hooks_ejecutables` | `git ls-files -s .githooks` | `100755` en `pre-commit` y `commit-msg`; `100644` en `commit-msg.mjs` |
  | `indice_completo` | `git diff --cached --name-only` | Exactamente: `.claude/agents/{planner,developer,reviewer}.md`, `.claude/hooks/{lib,guard-shell,protect-files,stop-gate}.mjs`, `.claude/settings.json`, `.githooks/{pre-commit,commit-msg,commit-msg.mjs}`, `.github/workflows/ci.yml`, `.github/ISSUE_TEMPLATE/bug.yml`, `.github/pull_request_template.md`, `.gitignore`, `CLAUDE.md`, `docs/GOVERNANCE.md`, `docs/MVP.md`, `docs/plans/_TEMPLATE.md`, `docs/plans/000-governance.md`, `scripts/quality-gate.mjs` |
  | `agentes_con_modelo_y_color` | `Select-String .claude/agents/*.md -Pattern '^(model|color):'` | 2 coincidencias por agente (planner `opus`/`purple`, developer `sonnet`/`green`, reviewer `opus`/`orange`) |
  | `claude_md_columna_modelo` | `Select-String CLAUDE.md -Pattern '^\| Agente \| Modelo \|'` | 1 coincidencia |
  | `sin_settings_local` | `git diff --cached --name-only` | No contiene `.claude/settings.local.json` |

### T11 — Commit inicial de la governance (O)
- **Ficheros/módulos:** —
- **Depende de:** T10
- **Cubre:** CA10
- **Subtareas:**
  - [x] T11.1 `git commit -m "chore(governance): reglas, hooks, CI y flujo de agentes"`, con cuerpo opcional en español. Sin `--no-verify`, sin `-s` y sin ninguna línea de atribución. Se activan guard-shell (rama y plan válidos), pre-commit (gate en verde) y commit-msg (R9).
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `commit_inicial_cumple_r9` | `git log --format=%B -1 > msg.txt; node .githooks/commit-msg.mjs msg.txt` | 0 |
  | `commit_inicial_padre_main` | `git rev-parse HEAD~1` | Igual a `git rev-parse main` |
  | `commit_inicial_en_rama` | `git branch --show-current` | `chore/000-governance` |
  | `arbol_limpio` | `git status --porcelain` | Vacío |
  | `asunto_longitud` | Longitud de `git log --format=%s -1` | ≤ 72 (el asunto propuesto tiene 55) |

### T12 — Push, ajustes de GitHub y primer PR (U, con OK explícito para el push)
- **Ficheros/módulos:** remoto `devlitus/app-llama`
- **Depende de:** T11
- **Cubre:** CA11, y valida CA7 en la CI real
- **Subtareas:**
  - [ ] T12.1 Si no existe, añadir el remoto: `git remote add origin https://github.com/devlitus/app-llama.git`.
  - [ ] T12.2 `git push -u origin main` **primero**, para que `main` sea la rama por defecto del repo vacío. Si no lo fuera, ejecutar `gh repo edit devlitus/app-llama --default-branch main`.
  - [ ] T12.3 Activar **Automatically delete head branches** (Settings → General → Pull Requests), o con `gh api -X PATCH repos/devlitus/app-llama -F delete_branch_on_merge=true`.
  - [ ] T12.4 `git push -u origin chore/000-governance` y abrir el PR hacia `main`. Comprobar que `Mensajes de commit (R9)`, `Rama dedicada (R10)` y `Quality gate` están en verde en `windows-latest` y en `macos-latest`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `github_rama_por_defecto` | `gh api repos/devlitus/app-llama --jq .default_branch` | `main` |
  | `github_borra_ramas` | `gh api repos/devlitus/app-llama --jq .delete_branch_on_merge` | `true` |

> T12 necesita red y GitHub. No es un test unitario (R3 no aplica): es una comprobación de despliegue que se hace una sola vez.

## Fuera de alcance

- Protección de rama de `main` en GitHub (checks obligatorios, revisión), que figura en §1 de GOVERNANCE. Se configura en un plan aparte, una vez que los checks existan en `main`.
- Crear las etiquetas de GitHub de §5 de GOVERNANCE.
- Una suite automatizada (`node --test`) para los hooks dentro del quality gate. Aquí la verificación es manual y reproducible.
- Cambiar la detección `isGit("commit")` de guard-shell, que ya existía (ver riesgos).
- Validar que el asunto esté "en español": no se puede automatizar de forma fiable y queda como norma de revisión.
- Cualquier código de la app (fase 0 del MVP en adelante).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Sin `chmod +x` en el índice, los hooks no se ejecutan en los clones de macOS. | T10.3 y el test `hooks_ejecutables`. |
| Si se sube antes `chore/000-governance`, GitHub la toma como rama por defecto del repo vacío. | T12.2 sube `main` primero y lo comprueba con `github_rama_por_defecto`. |
| Las etiquetas de §5 no existen todavía, así que `gh issue create --label ...` del reviewer (T5) puede fallar. | Crearlas antes de T5 con los comandos de §5, o que el reviewer abra los issues sin etiquetas y se etiqueten después. |
| Los patrones de atribución pueden dar falsos positivos: una línea del cuerpo como `Stand-by: ...` o un texto con "generated by", y en guard-shell cualquier `xxx-by:` del comando, aunque no esté a principio de línea. | Bloquear de más es preferible a dejar pasar. Si molesta, se ajustan los patrones en un plan aparte. |
| La longitud se cuenta en unidades UTF-16: un asunto con acentos en forma NFD cuenta más caracteres que los visibles, y 🤖 cuenta 2. | El límite de 72 es holgado. Si da falsos positivos, se normaliza a NFC en un plan posterior. |
| `isGit("commit")` también coincide con comandos como `git log --grep commit` o `git commit-tree`. Con R10 y R9 aumentan los bloqueos falsos. | Es aceptable porque solo bloquea, nunca deja pasar de más. Se ajusta en un plan aparte si hace falta. |
| El guard-shell de la sesión intercepta los comandos de prueba que contienen `git commit` y atribución. | T7 y T8 los ejecuta el usuario en su terminal, con el JSON en un fichero. |
| La CI ejecuta R9 y R10 dos veces (una por SO de la matriz). | Es inocuo y determinista. Se puede mover a un job aparte más adelante. |

## Preguntas abiertas

- Ninguna.
