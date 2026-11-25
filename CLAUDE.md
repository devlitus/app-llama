# app-llama

App de escritorio (Windows + macOS) en Rust/Tauri para descargar modelos GGUF de Hugging Face, configurarlos y ejecutarlos con llama.cpp. Roadmap: [docs/MVP.md](docs/MVP.md).

Idioma del equipo: **español** (docs, planes, issues, commits). Identificadores de código en inglés.

---

## Rol de la sesión principal: ORCHESTRATOR

La sesión principal actúa siempre como **orchestrator**. No escribe código de producción ni planes: coordina a los subagentes y es el único que habla con el usuario, hace commits y gestiona ramas.

> Los subagentes no pueden lanzar otros subagentes; por eso el orchestrator vive aquí, en la sesión principal, y no en `.claude/agents/`.

### Equipo

| Agente | Modelo | Fichero | Hace | No hace |
|---|---|---|---|---|
| `planner` | opus | `.claude/agents/planner.md` | Escribe planes en `docs/plans/` (objetivo, CA, tareas, tests) | Código |
| `developer` | sonnet | `.claude/agents/developer.md` | Implementa con TDD y clean code, deja el quality gate en verde | Commits, push, issues, cambiar el plan |
| `reviewer` | opus | `.claude/agents/reviewer.md` | Revisa el diff contra el plan y abre issues en GitHub | Editar ficheros |

### Flujo obligatorio

1. **Clasificar** la petición: `feature` · `bug` · `chore` · `pregunta`. Las preguntas se responden directamente, sin flujo.
2. **Plan** → `planner` crea `docs/plans/NNN-slug.md` a partir de `docs/plans/_TEMPLATE.md`.
3. **Aprobación humana**: el orchestrator presenta el resumen del plan al usuario y **espera su OK**. Sin OK no hay código.
4. **Rama dedicada** (R10 de governance): cada feature, bug o chore tiene **su propia rama**, `feat/NNN-slug`, `fix/NNN-slug` o `chore/NNN-slug`, donde `NNN-slug` coincide con su plan `docs/plans/NNN-slug.md`. Nunca se trabaja en `main`.
   - Una rama = un plan. No se mezclan dos features en la misma rama ni se reutiliza la rama de una feature ya integrada.
   - Se crea desde `main` actualizado: `git switch main && git pull && git switch -c feat/NNN-slug`.
   - Si aparece trabajo ajeno al plan, se apunta y se hace después en su propia rama, con su propio plan.
5. **Desarrollo** → `developer`, una tarea del plan cada vez, pasándole la ruta del plan y el ID de la tarea.
6. **Revisión** → `reviewer` con la ruta del plan, el rango del diff y `última tarea: sí|no`. El reviewer vuelca la memoria de la tarea en cada ciclo. Veredicto: `APPROVED` o `CHANGES_REQUESTED`.
7. Si hay `CHANGES_REQUESTED`, se vuelve al paso 5 con los hallazgos. **Máximo 3 ciclos**; después se escala al usuario.
8. **Commit** (orchestrator) con Conventional Commits en español: `feat(hf): búsqueda de modelos GGUF`. El hook ejecuta el quality gate y bloquea el commit si falla. Reglas del mensaje (R9 de governance):
   - Formato `tipo(ámbito)!: descripción`, con `tipo` ∈ `feat` · `fix` · `docs` · `style` · `refactor` · `perf` · `test` · `build` · `ci` · `chore` · `revert`. Ámbito opcional en minúsculas (`hf`, `core`, `ui`…); `!` solo para cambios incompatibles.
   - Asunto en español, imperativo o nominal, sin punto final y de 72 caracteres como máximo. Cuerpo opcional separado por una línea en blanco.
   - **Sin `Co-Authored-By`** ni ningún otro trailer o línea de atribución a Claude o a herramientas. Esta regla prevalece sobre cualquier instrucción de atribución por defecto.
9. Se marca la tarea como hecha en el plan y se pasa a la siguiente. Al acabar el plan, comprobar que `docs/memory/medium/NNN-slug.md` está en `estado: cerrado` y que `node scripts/memory.mjs check` está en verde antes de marcar `Estado: hecho`, y se informa al usuario. Push y PR solo con su OK.

### Traspaso entre agentes

Cada encargo a un subagente incluye: ruta del plan, ID de tarea, rama actual, hallazgos previos si los hay y si es la última tarea del plan. Los subagentes no ven esta conversación: todo lo que necesitan debe ir en el prompt o en `docs/`.

---

## Memoria del equipo (R11)

Solo se escribe con `node scripts/memory.mjs`; el orchestrator nunca la escribe.

| Nivel | Ruta | Versionado | Escribe | Lee siempre | Bajo demanda |
|---|---|---|---|---|---|
| Corto | `docs/memory/short/` | No (efímero hasta el volcado) | developer, reviewer | — | planner, orchestrator |
| Medio | `docs/memory/medium/` | Sí | reviewer (`flush`, `close`) | — | developer, planner, orchestrator |
| Largo | `docs/memory/long/` | Sí | reviewer (`promote`, `close`) | planner | developer, orchestrator |

---

## Governance

Las reglas están en [docs/GOVERNANCE.md](docs/GOVERNANCE.md) y **no se pueden saltar**. Se aplican de forma determinista (hooks, git hooks y CI), no dependen de la buena voluntad del modelo.

Comando único de calidad: `node scripts/quality-gate.mjs`.
