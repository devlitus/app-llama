# Governance

Reglas innegociables del proyecto. Se aplican **por máquina**, no por confianza: si una regla no está automatizada, se automatiza o no es una regla.

## 1. Capas de aplicación

| Capa | Dónde | Qué bloquea |
|---|---|---|
| Hooks de Claude Code | `.claude/settings.json` + `.claude/hooks/` | Commits con gate en rojo, `--no-verify`, push a `main`, push forzado, terminar el turno con código en rojo, editar ficheros de governance, commits con líneas de atribución y commits en ramas sin plan, escribir la memoria fuera de scripts/memory.mjs, que developer/reviewer terminen sin registrar o volcar su sesión |
| Git hooks `pre-commit` y `commit-msg` | `.githooks/` | Cualquier commit (humano o agente) con gate en rojo o mensaje que incumpla R9 |
| CI | `.github/workflows/ci.yml` | PRs con gate en rojo en Windows o macOS |
| Protección de rama | GitHub → Settings → Branches | Merge a `main` sin CI verde ni revisión |

La CI con protección de rama es la autoridad final: las capas locales se pueden desactivar en una máquina, la CI no.

## 2. Quality gate

Un único comando, idéntico en local, en hooks y en CI:

```
node scripts/quality-gate.mjs
```

| Paso | Comando | Se ejecuta si existe |
|---|---|---|
| Formato Rust | `cargo fmt --all -- --check` | `Cargo.toml` |
| Lint Rust | `cargo clippy --workspace --all-targets --locked -- -D warnings` | `Cargo.toml` |
| Tests Rust | `cargo test --workspace --locked` | `Cargo.toml` |
| Lint UI | `npm --prefix ui run lint` | `ui/package.json` |
| Tipos UI | `npm --prefix ui run check` | script `check` |
| Tests UI | `npm --prefix ui test` | `ui/package.json` |
| Tests de scripts | `node --test "scripts/**/*.test.mjs"` | `scripts/memory.mjs` |
| Memoria (R11) | `node scripts/memory.mjs check` | `scripts/memory.mjs` |
| Patrones prohibidos | escaneo propio | siempre |
| Secretos (R6) | escaneo propio de código, scripts/ y docs/memory/ | siempre |

Si existe `ui/package.json`, los scripts `lint` y `test` son obligatorios.

## 3. Reglas

### R1 — Gate en verde
Ningún commit ni fin de tarea con el gate en rojo. Prohibido `--no-verify` y prohibido cambiar `core.hooksPath`.

### R2 — Sin silenciar al linter ni a los tests
Prohibido en código fuente: `#[allow(...)]`, `#[ignore]`, `eslint-disable`, `@ts-ignore`, `@ts-nocheck`, `.only(`, `.skip(`, `dbg!`, `todo!`, `unimplemented!`. Si una excepción es imprescindible, se configura en `Cargo.toml` / config de ESLint con justificación y aprobación del usuario.

### R3 — Determinismo
- **Builds reproducibles:** `Cargo.lock` y `package-lock.json` versionados; `--locked` y `npm ci` en CI; toolchain fijada en `rust-toolchain.toml`.
- **Tests deterministas:** sin red, sin reloj real, sin aleatoriedad sin semilla fija, sin depender del orden de ejecución ni de ficheros fuera de un directorio temporal. HTTP con mocks (servidor local de test), tiempo inyectado mediante trait/clock.
- **Mismo resultado en Windows y macOS:** rutas con `Path`/`PathBuf`, nunca concatenando strings con `/` o `\`.

### R4 — Tests unitarios obligatorios
Toda tarea del plan define sus tests y el developer los escribe **antes** del código (TDD). Un bug arreglado incluye un test que lo reproduce.

### R5 — Flujo
Plan aprobado por el usuario → rama dedicada (R10) → desarrollo → revisión `APPROVED` → commit. Nunca se trabaja en `main`. Push y PR solo con OK del usuario.

### R6 — Seguridad
Ningún secreto en el repo (tokens de HF, certificados de firma). Los tokens del usuario van al llavero del sistema (`keyring`). Las descargas se verifican (tamaño + SHA256 cuando HF lo publica).

### R7 — Clean code
- Funciones pequeñas con una responsabilidad; nombres que explican la intención.
- Errores tipados (`thiserror`) en librerías; nada de `unwrap()`/`expect()` fuera de tests.
- `core` no depende de Tauri ni de la UI.
- Sin código muerto ni comentarios que repiten el código.

### R8 — Ficheros protegidos
Los agentes no pueden editar: `.claude/settings.json`, `.claude/hooks/**`, `.githooks/**`, `scripts/quality-gate.mjs`, `scripts/memory.mjs`, `scripts/memory.test.mjs`, `scripts/lib/**`, `.github/workflows/**`, `docs/GOVERNANCE.md`. Los cambios a la governance los hace el usuario a mano.

### R9 — Mensajes de commit
- [Conventional Commits](https://www.conventionalcommits.org/es/v1.0.0/): `tipo(ámbito)!: descripción`.
- `tipo` ∈ `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. Ámbito opcional en minúsculas (`hf`, `core`, `ui`…); `!` solo para cambios incompatibles.
- Asunto en español, sin punto final, máximo 72 caracteres. Cuerpo opcional separado por una línea en blanco.
- **Prohibida cualquier línea de atribución**: trailers `*-by:` (`Co-Authored-By`, `Signed-off-by`…), `Generated with/by`, `noreply@anthropic.com` y 🤖.
- `fixup!`/`squash!`/`amend!` se permiten en local, pero se integran con `git rebase --autosquash` antes del PR: la CI los rechaza.
- Lo aplica `.githooks/commit-msg` en local y la CI sobre los commits del PR.

### R10 — Una rama dedicada por feature
- Cada feature, bug o chore se desarrolla en **su propia rama**: `feat/NNN-slug`, `fix/NNN-slug` o `chore/NNN-slug`.
- `NNN-slug` coincide exactamente con su plan `docs/plans/NNN-slug.md`. Una rama = un plan.
- Prohibido mezclar features en una rama o reutilizar la rama de una feature ya integrada.
- La rama nace de `main` actualizado y se integra por PR. Tras el merge, se borra.
- Lo aplica el hook de Claude Code en cada commit y la CI en cada PR.

### R11 — Memoria del equipo
- Tres niveles en `docs/memory/`: `short/` (una entrada efímera por sesión de `developer` o `reviewer`, no versionada), `medium/NNN-slug.md` (memoria del plan: una sección por tarea con sus ciclos de revisión; al cerrar el plan se compacta en "Patrones del plan") y `long/decisiones.md` + `long/errores.md` (patrones presentes en 3 planes cerrados o más; 98 líneas como máximo en total).
- La memoria solo se escribe con `node scripts/memory.mjs`, desde la herramienta Bash y con la forma exacta de comando. Nadie la edita a mano salvo el usuario. `session`: `developer` y `reviewer`, cada uno con su propia identidad. `flush`, `close` y `promote`: `reviewer`. El orchestrator y el planner solo la leen; el planner lee siempre `long/`.
- Un `developer` no termina sin su entrada de sesión. Un `reviewer` no termina sin la suya ni sin volcar (`flush`) las sesiones de la tarea revisada en el medio plazo, sea cual sea el veredicto (hook `SubagentStop`). Los campos deterministas (fecha, rama, plan, tarea, ficheros cambiados, resultado del gate) los rellena el script, nunca el agente.
- Con `APPROVED` en la última tarea, el reviewer cierra la memoria del plan (`close`). Un plan con `Estado: hecho` no puede tener su memoria abierta (`memory.mjs check`, en el gate).
- Un error de memoria enlaza al issue (`#N`) y no lo copia: una sola línea de 200 caracteres como máximo.
- Sin secretos en la memoria (R6): `memory.mjs` rechaza la entrada y el gate escanea `docs/memory/`.

## 4. Definition of Done

- [ ] Todos los CA del plan cumplidos.
- [ ] Tests unitarios del plan escritos y en verde.
- [ ] `node scripts/quality-gate.mjs` en verde.
- [ ] Reviewer: `APPROVED`, y los bugs encontrados tienen su issue.
- [ ] Tarea marcada como hecha en el plan.
- [ ] Sesiones registradas y volcadas en la memoria del plan; si es la última tarea, memoria cerrada (R11).
- [ ] Commit con Conventional Commits y sin Co-Authored-By (R9).

## 5. Etiquetas de GitHub para issues del reviewer

`bug`, `review`, `severity:blocker`, `severity:major`, `severity:minor`. Se crean una vez con:

```
gh label create bug --color d73a4a --force
gh label create review --color 5319e7 --force
gh label create severity:blocker --color b60205 --force
gh label create severity:major --color d93f0b --force
gh label create severity:minor --color fbca04 --force
```
