# 001 — Memoria del equipo de agentes (corto, medio y largo plazo)

- **Tipo:** chore
- **Estado:** aprobado
- **Rama:** chore/001-memoria
- **Fase del roadmap:** previa a la fase 0 (Esqueleto) de [docs/MVP.md](../MVP.md), igual que el plan 000

## Objetivo

Que el equipo de agentes no repita decisiones ni errores:

- Cada sesión de `developer` y `reviewer` deja una entrada obligatoria.
- Cada revisión vuelca las entradas de su tarea en la memoria del plan.
- Los patrones que se repiten en 3 planes o más le llegan al planner antes de planificar.

Todo se genera con un script determinista, nunca a mano.

## Contexto

- **Precondición:** el plan 000 está cerrado e integrado en `main` (R9, R10, `commit-msg` y guard-shell parcheado). Este plan se ejecuta en `chore/001-memoria`, una rama creada desde `main` actualizado.
- **Estado actual relevante:**
  - **`.claude/hooks/lib.mjs`** exporta `PROJECT_DIR`, `readHookInput`, `block` (exit 2 + stderr), `git`, `PROTECTED_PATHS`, `toProjectRelative` e `isProtected`.
  - **`.claude/hooks/protect-files.mjs`** (PreToolUse `Edit|Write|NotebookEdit` y `Bash|PowerShell`) bloquea las escrituras en ficheros R8. En shell busca operadores de escritura junto a una ruta protegida.
  - **Hooks de parada:** `.claude/hooks/stop-gate.mjs` (Stop) solo actúa en la sesión principal. No hay hooks `SubagentStart` ni `SubagentStop`.
  - **`scripts/quality-gate.mjs`:** el escaneo "Patrones prohibidos" solo cubre R2 en `crates`, `src-tauri/src` y `ui/src`. No escanea secretos (R6) ni ejecuta tests de Node.
  - **`.gitignore`** no es un fichero protegido. Hoy ignora `target/`, `node_modules/`, `dist/`, `.env*`, `.claude/settings.local.json` y `*.gguf`.
  - **Agentes:**
    - el `reviewer` tiene `tools: Read, Grep, Glob, Bash` y "Nunca modificas ficheros";
    - el `developer` tiene un cierre obligatorio de 2 pasos;
    - el `planner` no lee ninguna memoria.
- **Documentación oficial de hooks verificada** (https://code.claude.com/docs/en/hooks, 2026-11-25):
  - **Campos comunes `agent_id` y `agent_type`:** `agent_type` está "Present when the session uses `--agent` or the hook fires inside a subagent". Los hooks de settings también se ejecutan dentro de los subagentes, con esos campos.
  - **Cómo se distingue al subagente:** en PreToolUse, por `agent_type`. La sesión principal (orchestrator) no lo trae.
  - **`SubagentStart`:** recibe `agent_id` y `agent_type`, y su matcher filtra por tipo de agente. No bloquea, pero puede inyectar `additionalContext`.
  - **`SubagentStop`:**
    - recibe `stop_hook_active`, `agent_id`, `agent_type`, `agent_transcript_path` y `last_assistant_message`;
    - su matcher filtra por tipo, y un matcher con nombres no casa con un `agent_type` vacío;
    - exit 2 o `decision: "block"` impiden que el subagente termine y le entregan `reason`;
    - hay un tope de 8 continuaciones seguidas, que se reinicia cada vez que se usa una herramienta.
  - **`last_assistant_message`:** con `SubagentHandback` no contiene el informe entregado, así que no se usa.
- **R8:** solo el usuario modifica `.claude/settings.json`, `.claude/hooks/**`, `docs/GOVERNANCE.md`, `scripts/quality-gate.mjs`, `.githooks/**` y `.github/workflows/**`.
  - La lógica de los hooks se escribe con TDD en módulos no protegidos (`scripts/lib/`).
  - Los ficheros protegidos quedan como envoltorios finos, con su contenido exacto en las tareas **U**.

## Decisiones

- **D1 — Sesiones efímeras.**
  - `docs/memory/short/` está en `.gitignore`: no se versiona ni se archiva.
  - Las sesiones viven desde que el agente termina hasta que el reviewer de esa tarea las vuelca en el medio plazo y las borra.
  - Por eso ninguna comprobación que corra en la CI (`check`) lee `short/`.
- **D2 — Volcado por tarea en cada ciclo de revisión.**
  - Al terminar, el reviewer escribe su sesión y ejecuta `flush --task Tn`. `flush` vuelca en `docs/memory/medium/NNN-slug.md` todas las sesiones pendientes de esa tarea (las de developer y la suya) y después las borra.
  - Pasa igual con `APPROVED` que con `CHANGES_REQUESTED`.
  - **Una sección por tarea que acumula ciclos**, en lugar de una sección por ciclo, por tres motivos:
    - Quien lee el medio plazo busca "qué pasó en T3". Una sección por ciclo dispersaría la tarea en N bloques.
    - Agrupar por slug dentro de la tarea muestra que un error se repitió entre ciclos (`(C1, C2)`), que es justo la señal útil.
    - El detalle de cada ciclo no se pierde: hay una fila por ciclo en la tabla "Ciclos" y una línea de resumen por sesión.
  - El ciclo `k` de una tarea es su k-ésimo volcado.
- **D3 — Solo el reviewer escribe el medio plazo** (`flush`, `close`) **y el largo plazo** (`promote`, también dentro de `close`).
- **D4 — Cierre del plan = compactar + promover.**
  - Cuando el reviewer da `APPROVED` a la última tarea, ejecuta `close`, que hace dos cosas:
    - compacta el medio plazo: quita los resúmenes y las decisiones y errores por tarea, y los sustituye por una sección única "Patrones del plan" agregada por slug, conservando la tabla de ciclos de cada tarea;
    - regenera el largo plazo.
  - Compactar es necesario: un plan de 10–15 tareas con 2 ciclos genera unas 300 líneas de detalle. El historial completo queda en git.
  - `promote` solo cuenta los planes con `estado: cerrado`, para que un plan a medias no cuente como uno de los 3.
- **D5 — La memoria solo se escribe con `node scripts/memory.mjs`.**
  - `Edit`, `Write`, `NotebookEdit` y los comandos de shell que escriben en `docs/memory/` se bloquean para todos, incluida la sesión principal. Solo el usuario la edita a mano.
- **D6 — Permisos por agente con `agent_type` y `agent_id`:**

  | Subcomando | Quién | Requisito extra |
  |---|---|---|
  | `session` | `developer`, `reviewer` | `--agent` == `agent_type` y `--agent-id` == `agent_id` |
  | `flush` | `reviewer` | `--agent-id` == `agent_id` |
  | `close`, `promote` | `reviewer` | — |
  | `check`, `tags`, `help` | cualquiera | — |

  - **Si T13.2 demuestra que `agent_type` no llega a PreToolUse,** las escrituras quedan bloqueadas para todos: el fallo es visible y no deja pasar de más. La alternativa serían hooks en el frontmatter de cada agente, en un plan aparte.
- **D7 — Entrega del `agent_id`.**
  - `SubagentStart` inyecta en `developer` y `reviewer` su `agent_id` y las órdenes exactas.
  - El `reason` de `SubagentStop` las repite, como respaldo.
- **D8 — Obligación (`SubagentStop`):**
  - **`developer`:** se le bloquea si no existe en `short/` una sesión con su `agent_id`.
  - **`reviewer`, si su sesión sigue en `short/`:** se le bloquea con la orden `flush` de la tarea de esa sesión.
  - **`reviewer`, si su sesión no está en `short/`:** se busca su `agent_id` en la columna Reviewer del medio plazo del plan.
    - Si no aparece, se le bloquea con la orden `session`.
    - Si aparece pero quedan sesiones de esa tarea en `short/`, se le bloquea con la orden `flush`.
  - Siempre se bloquea, aunque `stop_hook_active` sea `true`; el tope de 8 continuaciones garantiza que termina.
  - Se permite terminar si la rama no corresponde a un plan, y nunca se bloquea a otros agentes.
- **D9 — La lógica de los hooks vive en `scripts/lib/memory-policy.mjs`**, testable. Los hooks protegidos solo la importan. `protect-files` la importa de forma dinámica con `catch`, para que R8 siga activo aunque el módulo falte.
- **D10 — Entrada de `session` por stdin y slug obligatorio.**
  - Las líneas tienen la forma `resumen:`, `decision: [slug]` y `error: [slug]`, y se pasan con un heredoc `<<'EOF'`.
  - El slug es la clave determinista para detectar patrones.
  - `tags` lista los slugs existentes y `SubagentStart` los inyecta para que se reutilicen.
- **D11 — Campos deterministas, que rellena el script:**
  - **fecha:** UTC, en formato `YYYY-MM-DD`;
  - **rama:** `git branch --show-current`;
  - **plan:** se deduce de la rama con el patrón R10 `^(feat|fix|chore)/(\d{3}-[a-z0-9]+(?:-[a-z0-9]+)*)$`;
  - **tarea:** se valida contra un encabezado `### Tn ` del plan;
  - **identidad:** agente, `agent_id` y `n`;
  - **ficheros cambiados:** `git -c core.quotePath=false status --porcelain=v1 -z --untracked-files=all`, sin `docs/memory/`;
  - **gate:** `session` ejecuta `node scripts/quality-gate.mjs`. Esto está aceptado por el usuario (antes P1); se optimizará en otro plan cuando entre Rust.
- **D12 — Largo plazo** (los números los aceptó el usuario; antes P2):
  - un slug se promueve si aparece en los "Patrones del plan" de **≥ 3 planes cerrados**;
  - **40 entradas por fichero**, 49 líneas por fichero y 98 en total;
  - desempate: nº de planes desc., plan más reciente desc. y slug asc.;
  - los dos ficheros se regeneran enteros con cada `promote`.
- **D13 — `check` solo valida ficheros versionados**, porque en la CI no hay sesiones:
  - el medio plazo está en formato canónico (`render(parse(x)) == x`);
  - los ciclos son consecutivos;
  - un medio plazo `cerrado` tiene el último ciclo de cada tarea en `APPROVED`;
  - el largo plazo coincide con su regeneración;
  - un plan con `Estado: hecho` no tiene el medio plazo `abierto`;
  - `.gitignore` contiene `docs/memory/short/`.
- **D14 — `scripts/memory.mjs`, `scripts/memory.test.mjs` y `scripts/lib/**` pasan a protegidos (R8) en T9.**
  - Son el mecanismo que aplica R11. Si un agente pudiera editarlos, podría falsear el gate o relajar los permisos.
  - Los tests también se protegen, para que la especificación no se debilite.
- **D15 — Secretos (R6):**
  - `scripts/lib/secrets.mjs` concentra los patrones y lo usan dos sitios:
    - el gate, con un paso "Secretos" sobre el código, `scripts/` y `docs/memory/` (localmente incluye `short/`);
    - `session`, que rechaza la entrada antes de escribirla.
  - R2 no se amplía a `docs/memory/`.
- **D16 — Tests de `scripts/` en el gate** con `node --test "scripts/**/*.test.mjs"`. Requiere Node ≥ 22; la CI usa Node 24.
- **D17 — Escritura solo desde Bash y con una forma exacta de comando.**
  - El heredoc solo se admite en `session`; el resto va en una sola línea.
  - No se admiten encadenados, redirecciones ni rutas alternativas al script.
  - PowerShell solo puede usar los subcomandos de lectura.
- **D18 — Reparto de ficheros no protegidos:** `.claude/agents/*.md`, `CLAUDE.md` y `.gitignore` los edita el orchestrator (T8), porque las reglas del developer le prohíben `.claude/`. El reviewer no necesita `Write` ni `Edit`: le basta Bash.
- **D19 — Un error enlaza a su issue y no lo copia:** una línea de ≤ 200 caracteres. Los `#N` se extraen y se agregan.

## Criterios de aceptación

- **CA1 (sesión):**
  - **Dado** la rama `chore/001-memoria`, un plan con `### T2 ` y una entrada válida,
  - **cuando** se ejecuta `session --agent developer --agent-id a1 --task T2`,
  - **entonces**:
    - sale con 0;
    - crea `docs/memory/short/<fecha UTC>-001-memoria-T2-developer-<n>.md` con el formato exacto de "Formatos", que incluye la rama, los ficheros cambiados (ordenados y sin `docs/memory/`) y el gate;
    - escribe la ruta en stdout.
- **CA2 (entrada inválida):**
  - **Dado** una entrada sin `resumen`, con una clave desconocida, sin slug o con un slug inválido, con un texto de más de 200 caracteres, con más de 10 decisiones o 10 errores, o con un secreto,
  - **cuando** se ejecuta `session`,
  - **entonces**:
    - sale con 2;
    - stderr empieza por `[R11] `;
    - no se escribe ningún fichero y no se ejecuta el gate.
- **CA3 (precondiciones):**
  - **Dado** una rama que no sigue R10, un plan o una tarea inexistentes, o un `agent_id` ya presente en `short/`,
  - **cuando** se ejecuta `session`,
  - **entonces** sale con 3 y no escribe nada.
  - Si el argumento está mal formado, sale con 1.
- **CA4 (volcado):**
  - **Dado** en `short/` 1 o más sesiones de developer de `T2` y la sesión del reviewer `r1` de `T2`,
  - **cuando** se ejecuta `flush --task T2 --agent-id r1`,
  - **entonces**:
    - el medio plazo del plan queda con la sección `## T2` y un ciclo nuevo `C<k>` (k = ciclos previos + 1), con el formato abierto de "Formatos";
    - las sesiones de `T2` se borran y las de otras tareas siguen en `short/`.
  - Pasa igual con `APPROVED` y con `CHANGES_REQUESTED`.
  - Repetir el volcado tras un fallo parcial no duplica el ciclo.
- **CA5 (cierre):**
  - **Dado** un medio plazo abierto en el que todas las tareas tienen su último ciclo `APPROVED` y no quedan sesiones del plan en `short/`,
  - **cuando** el reviewer ejecuta `close`,
  - **entonces**:
    - el medio plazo pasa a `estado: cerrado` con el formato compacto;
    - se regenera el largo plazo;
    - una segunda ejecución deja todo idéntico byte a byte.
  - Si alguna tarea tiene el último ciclo en `CHANGES_REQUESTED` o quedan sesiones, sale con 3 sin escribir nada.
- **CA6 (largo plazo):**
  - **Dado** varios medios plazos,
  - **cuando** se ejecuta `promote`,
  - **entonces**:
    - solo se promueven los slugs presentes en ≥ 3 planes **cerrados**;
    - cada fichero tiene ≤ 40 entradas y ≤ 49 líneas, y entre los dos suman ≤ 100.
- **CA7 (check):**
  - **Dado** un medio plazo editado a mano fuera del formato canónico, con ciclos no consecutivos, `cerrado` con una tarea sin `APPROVED`, un largo plazo desactualizado, un plan `hecho` con el medio plazo `abierto` o un `.gitignore` sin `docs/memory/short/`,
  - **cuando** se ejecuta `check`,
  - **entonces** sale con 4 y escribe una línea `[R11] <ruta>: <motivo>` por incidencia.
  - Sale con 0 cuando no hay memoria y cuando la memoria es coherente.
  - No lee `docs/memory/short/` y tolera CRLF.
- **CA8 (permisos):**
  - **Dado** `protect-files` parcheado,
  - **cuando** cualquier agente o la sesión principal hace `Edit`/`Write` en `docs/memory/**`, o ejecuta un comando de shell que escribe ahí,
  - **entonces** sale con 2 y el motivo contiene `R11`.
  - `session`, `flush`, `close` y `promote` solo pasan para los agentes y las identidades de D6.
  - `check` y `tags` pasan para todos.
- **CA9 (obligación):**
  - **Dado** `SubagentStop` registrado,
  - **cuando** un `developer` intenta terminar sin su sesión, o un `reviewer` lo intenta sin su sesión o con sesiones de su tarea sin volcar,
  - **entonces** se le bloquea, y el motivo contiene la orden exacta (`session …` o `flush --task <Tn> --agent-id <id>`) con su `agent_id`.
  - Cumplida la condición, termina.
- **CA10 (contexto inicial):**
  - **Dado** `SubagentStart` registrado,
  - **cuando** arranca un `planner`,
  - **entonces** su contexto contiene el largo plazo, o la frase `[R11] La memoria a largo plazo está vacía.`
  - Si arranca un `developer` o un `reviewer`, su contexto contiene su `agent_id`, las órdenes exactas (el reviewer también `flush` y `close`) y los slugs existentes.
- **CA11 (secretos):**
  - **Dado** el gate parcheado,
  - **cuando** un fichero de `docs/memory/`, `scripts/` o el código contiene un token de HF, GitHub o Anthropic, una clave AWS o una clave privada,
  - **entonces** el paso `Secretos` falla con `<ruta>:<línea> — posible secreto (<tipo>)`.
- **CA12 (tests en el gate):**
  - **Dado** el gate parcheado,
  - **cuando** falla un test de `scripts/**/*.test.mjs` o `memory.mjs check`,
  - **entonces** el gate sale con 1.
  - Si todo está en verde, muestra `✔ Tests scripts`, `✔ Memoria (R11)` y `✔ Secretos`.
- **CA13 (governance):**
  - **Dado** `GOVERNANCE.md` parcheado,
  - **entonces**:
    - existe `### R11 — Memoria del equipo` después de R10;
    - R8 enumera `scripts/memory.mjs`, `scripts/memory.test.mjs` y `scripts/lib/**`;
    - §2 enumera `Tests scripts`, `Memoria (R11)` y `Secretos`.
- **CA14 (agentes, flujo y .gitignore):**
  - **Dado** T8 aplicada,
  - **entonces**:
    - `developer.md` tiene el paso de memoria;
    - `reviewer.md` permite escribir solo vía `memory.mjs` y documenta `session`, `flush` y `close`;
    - `planner.md` lee siempre `long/`;
    - `CLAUDE.md` tiene la sección de memoria y `última tarea: sí|no`;
    - `.gitignore` contiene `docs/memory/short/`.
- **CA15 (protección):**
  - **Dado** T9 aplicada,
  - **cuando** un agente intenta editar `scripts/memory.mjs`, `scripts/memory.test.mjs` o `scripts/lib/*`,
  - **entonces** `protect-files` sale con 2 por R8.
- **CA16 (dogfooding):**
  - **Dado** el plan terminado,
  - **entonces**:
    - `docs/memory/medium/001-memoria.md` existe con `estado: cerrado`;
    - `git ls-files docs/memory/short` está vacío;
    - no quedan sesiones de `001-memoria` en `short/`;
    - `check` y el gate están en verde;
    - T13.2 confirmó en vivo `agent_type` y `agent_id` en PreToolUse, `SubagentStart` y `SubagentStop`.

## Formatos y CLI (referencia normativa para T2–T7)

### Estructura

```
docs/memory/
├─ short/                  # efímero, en .gitignore: sesiones pendientes de volcar
├─ medium/NNN-slug.md      # uno por plan: abierto (por tarea) → cerrado (compacto)
└─ long/decisiones.md, long/errores.md
```

- Los directorios se crean bajo demanda.
- Los ficheros son UTF-8 con LF y un único `\n` final.
- Al leer se aceptan CRLF.
- Las escrituras del medio plazo y del largo plazo pasan por un fichero temporal en el mismo directorio y un `renameSync`.

### CLI

```
node scripts/memory.mjs session --agent <developer|reviewer> --agent-id <id> --task <Tn> [--veredicto <APPROVED|CHANGES_REQUESTED>]   (stdin)
node scripts/memory.mjs flush --task <Tn> --agent-id <id>
node scripts/memory.mjs close
node scripts/memory.mjs promote
node scripts/memory.mjs check
node scripts/memory.mjs tags
node scripts/memory.mjs help | --help
```

- `--agent-id` debe cumplir `^[A-Za-z0-9._-]{1,64}$` y `--task` debe cumplir `^T\d+$`.
- `--veredicto` es obligatorio con `reviewer` y está prohibido con `developer`.
- El plan se deduce de la rama.
- La raíz del repositorio se calcula con `import.meta.url` (`scripts/..`).
- Los errores van a stderr con el prefijo `[R11] `.

| Código | Significado |
|---|---|
| 0 | Éxito |
| 1 | Uso incorrecto (subcomando o argumentos) |
| 2 | Entrada de stdin inválida (incluye secretos) |
| 3 | Precondición: rama sin plan, plan o tarea inexistentes, `agent_id` duplicado, sesión del reviewer ausente o duplicada, plan ya cerrado, tareas sin `APPROVED`, sesiones pendientes, fichero ilegible |
| 4 | `check` encontró incumplimientos |

### Entrada de `session` (stdin)

```
resumen: <texto>                      exactamente 1, ≤ 300 caracteres
decision: [<slug>] <texto>            0–10
error: [<slug>] <texto>               0–10; puede contener #N
```

- Se ignoran las líneas vacías y los `\r` finales. Cualquier otra línea es un error que indica su número.
- El slug cumple `^[a-z0-9]+(-[a-z0-9]+)*$` y tiene ≤ 40 caracteres.
- El texto tiene entre 1 y 200 caracteres, sin espacios en los extremos.
- No se repite un slug del mismo tipo dentro de una sesión.
- Los `#\d+` de los errores se extraen sin duplicados y en orden ascendente.
- **Reglas de entrada** (enmienda tras T2). Todas lanzan `MemoryInputError` (código 2) con el prefijo `línea <n>: `:
  - **Formato estricto `clave: valor`:** la línea casa con `^([^\s:]+):(?: (.*))?$`. No se admiten espacios antes de la clave ni antes de `:`, y tras `:` va un espacio obligatorio (`resumen:a` es inválido). Los espacios sobrantes del valor se recortan.
  - **Slug:** tras `[slug]` va al menos un espacio o el fin del valor (`decision: [a]pegado` es inválido).
  - **El resumen no puede empezar por `#`:** ocupa una línea propia en la sesión y rompería el parseo de las secciones `## `. Las decisiones y los errores sí pueden empezar por `#` (`error: [x] #12 falla` da `issues:[12]`), porque se escriben tras `- **slug** — `.
  - La única excepción sin número de línea es `falta 'resumen'`, que se detecta al final.
  - El prefijo `[R11] ` no lo añade `MemoryInputError`: lo añade la CLI (T3) al escribir en stderr.

#### Contrato de `meta` (enmienda tras T2)

`renderSession(meta, input)` lo recibe y `parseSession(md)` lo devuelve. T3–T7 dependen de él:

```
{fecha, plan, tarea, agente, agentId, n, rama, gate: "verde"|"rojo", gateFailed?: string[], veredicto?: string, files: string[]}
```

- `agentId` se serializa como `agent_id` en el front matter.
- `gateFailed` solo existe cuando `gate` es `"rojo"`. Si el front matter pone `gate: rojo` a secas, `parseSession` devuelve `gateFailed: []`; con `gateFailed: []`, `renderSession` escribe `gate: rojo` a secas.
- `veredicto` solo existe para el reviewer.
- `files` se escribe en la sección "Ficheros cambiados", no en el front matter.

### Sesión (efímera): `short/<fecha>-<plan>-<tarea>-<agente>-<n>.md`

`n` es 1 más el máximo `n` que ya exista en `short/` con el mismo prefijo.

```
---
schema: memoria-sesion/1
fecha: 2026-11-25
plan: 001-memoria
tarea: T3
agente: reviewer
agent_id: r1
n: 1
rama: chore/001-memoria
gate: verde
veredicto: APPROVED
---

# Sesión 001-memoria · T3 · reviewer · 1

## Resumen

Revisa la CLI de session.

## Ficheros cambiados

- `scripts/memory.mjs`
- `scripts/memory.test.mjs`

## Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias.

## Errores

- **validacion-args** — No se valida el formato de --task. #12
```

- `veredicto:` solo se escribe para `reviewer`.
- Si el gate está en rojo, la línea es `gate: rojo (Tests Rust, Lint UI)`, con los pasos que aparecen marcados `✖` tras `──── Quality gate ────`.
- Las listas vacías se escriben como `- (ninguno)` (ficheros), `- (ninguna)` (decisiones) y `- (ninguno)` (errores).

### Medio plazo abierto: `medium/<plan>.md`

```
---
schema: memoria-plan/2
plan: 001-memoria
estado: abierto
---

# Memoria del plan 001-memoria

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-25 | 1 | r1 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | r2 | verde | APPROVED |

### Resúmenes

- C1 · developer · Implementa la CLI de session.
- C1 · reviewer · Falta validar el formato de --task.
- C2 · developer · Valida --task con ^T\d+$.
- C2 · reviewer · Revisa la corrección de --task.

### Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (C1)

### Errores

- **validacion-args** — No se valida el formato de --task. #12 (C1 · #12)
```

- **Secciones y tabla:**
  - Las secciones `## Tn` van en orden numérico de tarea.
  - En cada ciclo, `Developer` es el nº de sesiones de developer volcadas, `Reviewer` es el `agent_id` del reviewer, y `Fecha`, `Gate` y `Veredicto` salen de la sesión del reviewer. `Gate` solo dice `verde` o `rojo`.
- **Resúmenes:** dentro de cada ciclo, primero los de developer (por `n`) y después el del reviewer.
- **Decisiones y errores:**
  - Se agrupan por slug en orden alfabético.
  - El texto es el de la última ocurrencia, ordenando por ciclo y, dentro de cada ciclo, developer (por `n`) antes que reviewer.
  - Entre paréntesis van los ciclos en orden numérico y, en los errores, ` · ` seguido de la unión de los issues.
- **Listas vacías:** se escriben como `- (ninguna)` o `- (ninguno)`.

### Medio plazo cerrado (tras `close`)

```
---
schema: memoria-plan/2
plan: 001-memoria
estado: cerrado
---

# Memoria del plan 001-memoria

## Patrones del plan

### Decisiones

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (T3)

### Errores

- **validacion-args** — No se valida el formato de --task. #12 (T3 · #12)

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-25 | 1 | r1 | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | r2 | verde | APPROVED |
```

- "Patrones del plan" agrega por slug las decisiones y los errores de todas las tareas.
- El texto es el de la última ocurrencia, ordenando por nº de tarea y después por ciclo. Entre paréntesis van las tareas y, en los errores, los issues.
- Las tareas solo conservan la tabla "Ciclos".

### Largo plazo: `long/decisiones.md` y `long/errores.md`

```
---
schema: memoria-larga/1
tipo: decisiones
---

# Decisiones recurrentes

<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->

- **cli-sin-dependencias** — Parseo de argumentos a mano para no añadir dependencias. (planes: 001-memoria, 003-x, 005-y)
```

- En `errores.md` la cabecera lleva `tipo: errores` y el título es `# Errores recurrentes`.
- Solo cuentan los planes con `estado: cerrado`.
- El texto es el del plan con mayor `NNN`, y los planes se listan en orden ascendente.
- Si no hay entradas, se escribe `- (ninguna)` o `- (ninguno)`.
- Las entradas siguen el orden de D12.
- Las que no caben por el límite se informan en stdout como `descartado por límite: <tipo> <slug>`.

### `tags` y `check`

- **`tags`:**
  - escribe una línea `<decision|error> <slug> <planes>` por cada slug que aparezca en `medium/` (abiertos y cerrados), donde `<planes>` es el nº de planes en los que aparece;
  - las ordena por tipo, después por nº de planes (desc.) y después por slug;
  - sale con 0.
- **`check`:** ver D13. Si todo está bien, escribe `Memoria OK (<p> planes, <c> cerrados)` y sale con 0.

## Tareas

> Quién: **developer** = TDD con `node --test` en módulos no protegidos. **O** = orchestrator. **U** = usuario a mano (R8).
>
> Hasta T11, el developer y el reviewer ejecutan además `node --test "scripts/**/*.test.mjs"`.
>
> Reglas comunes de los tests (R3):
> - un directorio propio por test (`fs.mkdtempSync(join(os.tmpdir(), "mem-"))`), que se borra al terminar;
> - el reloj, git, el gate y stdin se inyectan;
> - sin red;
> - los tokens de ejemplo se construyen concatenando (`"hf_" + "A".repeat(34)`).

### T1 — Patrones de secretos compartidos (developer)
- **Ficheros/módulos:** `scripts/lib/secrets.mjs`, `scripts/lib/secrets.test.mjs`
- **Depende de:** —
- **Cubre:** CA11, CA2 (parte)
- **Subtareas:**
  - [x] T1.1 `SECRET_PATTERNS: {kind, re}[]`, en este orden:

    | `kind` | Patrón |
    |---|---|
    | `hugging-face` | `hf_[A-Za-z0-9]{30,}` |
    | `github` | `gh[pousr]_[A-Za-z0-9]{36,}` |
    | `github` | `github_pat_[A-Za-z0-9_]{22,}` |
    | `anthropic` | `sk-ant-[A-Za-z0-9_-]{20,}` |
    | `aws` | `AKIA[0-9A-Z]{16}` |
    | `private-key` | `-{5}BEGIN [A-Z ]*PRIVATE KEY-{5}` |
  - [x] T1.2 `findSecrets(text) → {line, kind}[]`, con las líneas numeradas desde 1 y admitiendo `\r\n`. El orden es por línea y, dentro de la línea, por el orden de los patrones.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `secrets_detecta_token_hf` | `"token = " + "hf_" + 34×A` | `[{line:1, kind:"hugging-face"}]` |
  | `secrets_ignora_hf_corto` | `"hf_abc"` | `[]` |
  | `secrets_detecta_github` | `"ghp_" + 36×a` | `kind:"github"` |
  | `secrets_detecta_github_pat` | `"github_pat_" + 22×a` | `kind:"github"` |
  | `secrets_detecta_anthropic` | `"sk-ant-" + 24×a` | `kind:"anthropic"` |
  | `secrets_detecta_aws` | `"AKIA" + "ABCDEFGHIJKLMNOP"` | `kind:"aws"` |
  | `secrets_detecta_clave_privada` | `"-----BEGIN RSA PRIVATE KEY-----"` y `"-----BEGIN PRIVATE KEY-----"` | `private-key` en cada caso |
  | `secrets_numera_lineas_crlf` | `"a\r\nb\nhf_"+34×A` | `line:3` |
  | `secrets_dos_en_una_linea` | Un token HF y una clave AWS en la misma línea | 2 resultados, primero `hugging-face` |
  | `secrets_texto_limpio` | `"Guarda el token de HF en keyring"` | `[]` |
  | `secrets_no_se_detecta_a_si_mismo` | `SECRET_PATTERNS.map(p => String(p.re)).join("\n")` | `[]` |

### T2 — Formato de sesión (developer)
- **Ficheros/módulos:** `scripts/lib/memory-entry.mjs`, `scripts/lib/memory-entry.test.mjs`
- **Depende de:** T1
- **Cubre:** CA1 (formato), CA2
- **Subtareas:**
  - [x] T2.1 `parseSessionInput(text) → {summary, decisions:[{slug,text}], errors:[{slug,text,issues:number[]}]}`. Lanza `MemoryInputError` (código 2) e indica el nº de línea cuando aplica. Usa `findSecrets`.
  - [x] T2.2 `formatDate(date)` (UTC) y `sessionFileName(meta)`.
  - [x] T2.3 `renderSession(meta, input)` con el formato exacto.
  - [x] T2.4 `parseSession(md) → {meta, input}`, la inversa de `renderSession`. Acepta CRLF y lanza error si falta el front matter o el `schema` no es `memoria-sesion/1`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `entrada_minima` | `"resumen: Implementa X"` | `{summary:"Implementa X", decisions:[], errors:[]}` |
  | `entrada_completa` | Resumen, 2 decisiones y `error: [validacion-args] falla #12` | Todos los campos rellenos, `issues:[12]` |
  | `entrada_crlf_y_vacias` | Entrada con `\r\n` y líneas vacías intercaladas | Igual que sin ellas |
  | `entrada_issues_ordenados_unicos` | `error: [x] ver #12, #7 y #12` | `issues:[7,12]` |
  | `entrada_sin_resumen` | `"decision: [a] b"` | Error `falta 'resumen'` |
  | `entrada_dos_resumenes` | 2 líneas `resumen:` | Error `solo se admite un 'resumen'` |
  | `entrada_clave_desconocida` | `"resumen: a\nnota: b"` | Error con `línea 2` y `clave desconocida 'nota'` |
  | `entrada_sin_slug` | `"resumen: a\ndecision: sin corchetes"` | Error con `slug` y `línea 2` |
  | `entrada_slug_invalido` | `decision: [Rutas_Win] x` | Error `slug inválido` |
  | `entrada_slug_limite` | Slug de 40 caracteres / de 41 | Válido / error |
  | `entrada_texto_limite` | Texto de 200 caracteres / de 201 | Válido / error `máximo 200` |
  | `entrada_resumen_limite` | Resumen de 300 caracteres / de 301 | Válido / error `máximo 300` |
  | `entrada_texto_vacio` | `decision: [a]   ` | Error `texto vacío` |
  | `entrada_slug_repetido` | 2 decisiones con `[a]` | Error `slug repetido 'a'` |
  | `entrada_slug_igual_distinto_tipo` | `decision: [a] x` y `error: [a] y` | Válido |
  | `entrada_demasiadas_decisiones` | 11 decisiones | Error `máximo 10` |
  | `entrada_con_secreto` | `"resumen: usa hf_"+34×A` | Error `posible secreto (hugging-face)` |
  | `fecha_utc` | `new Date("2026-11-25T23:30:00-05:00")` | `"2026-11-26"` |
  | `nombre_fichero_sesion` | `{fecha:"2026-11-25", plan:"001-memoria", tarea:"T2", agente:"developer", n:3}` | `"2026-11-25-001-memoria-T2-developer-3.md"` |
  | `render_sesion_exacto` | Meta y entrada del ejemplo de "Formatos" | Igual byte a byte |
  | `render_listas_vacias` | Sin ficheros, decisiones ni errores | `- (ninguno)`, `- (ninguna)`, `- (ninguno)` |
  | `render_gate_rojo` | `gate:"rojo"`, `gateFailed:["Tests Rust","Lint UI"]` | `gate: rojo (Tests Rust, Lint UI)` |
  | `render_developer_sin_veredicto` | Sesión de developer | No contiene `veredicto:` |
  | `parse_roundtrip` | `parseSession(renderSession(m, i))` | `{meta:m, input:i}` |
  | `parse_roundtrip_crlf` | El mismo render con CRLF | Mismo resultado |
  | `parse_sin_front_matter` | `"# hola"` | Lanza error |
  | `parse_schema_desconocido` | `schema: memoria-sesion/2` | Lanza error |

### T3 — CLI y `session` (developer)
- **Ficheros/módulos:** `scripts/memory.mjs`, `scripts/lib/memory-store.mjs`, `scripts/memory.test.mjs`
- **Depende de:** T2
- **Cubre:** CA1, CA2, CA3
- **Subtareas:**
  - [ ] T3.1 `memory-store.mjs`:
    - `listSessions(root) → {rel, meta, input}[]`, solo sobre `short/*.md`. Lanza error si una sesión es ilegible.
    - `nextN(root, prefix)`.
    - `findSessionByAgentId(root, id) → {rel, meta} | null`. Usa la línea exacta `agent_id: <id>` y no lanza error ante ficheros corruptos.
    - `sessionsOfTask(root, plan, task)`.
  - [ ] T3.2 `memory.mjs` exporta `run(argv, deps) → Promise<exitCode>`, con `deps = {root, now, git(args)→string|null, runGate()→{ok, failed[]}, readStdin()→string, out, err}`.
    - El punto de entrada solo se ejecuta si el módulo es el script principal.
    - El `runGate` real lanza `process.execPath scripts/quality-gate.mjs`, con `cwd = root` y `maxBuffer` de 64 MB.
  - [ ] T3.3 `session`, en este orden: argumentos (1) → rama y plan (3) → tarea (3) → stdin (2) → `agent_id` duplicado (3) → ficheros → gate → escritura → stdout.
  - [ ] T3.4 `help`/`--help` y subcomando desconocido. Los subcomandos de T4–T6 responden `[R11] subcomando aún no disponible` y salen con 1 hasta que se implementen.
- **Tests unitarios** (raíz con `docs/plans/001-memoria.md`, que contiene `### T2 — x`; git falso):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `session_crea_fichero` | Rama `chore/001-memoria`, `now = 2026-11-25T10:00Z`, stdin `resumen: a`, gate verde | 0; `docs/memory/short/2026-11-25-001-memoria-T2-developer-1.md` es igual a `renderSession(...)`, y esa ruta sale por stdout |
  | `session_incrementa_n` | Ya existe `…-T2-developer-1.md` | Crea `…-developer-2.md` |
  | `session_n_por_agente` | Ya existe `…-T2-developer-1.md` y la sesión es de `reviewer` | Crea `…-T2-reviewer-1.md` |
  | `session_ficheros_cambiados` | status `-z`: ` M scripts/a.mjs\0?? docs/memory/short/x.md\0R  new.md\0old.md\0?? con espacio.md\0` | `con espacio.md`, `new.md`, `scripts/a.mjs` |
  | `session_sin_cambios` | status vacío | `- (ninguno)` |
  | `session_gate_rojo` | `runGate → {ok:false, failed:["Tests Rust"]}` | 0; `gate: rojo (Tests Rust)` |
  | `session_reviewer_veredicto` | `--agent reviewer --veredicto CHANGES_REQUESTED` | `veredicto: CHANGES_REQUESTED` |
  | `session_rama_sin_plan` | Rama `main` | 3; `[R11] la rama 'main' no corresponde a un plan (R10)` |
  | `session_head_separado` | git devuelve `""` | 3 |
  | `session_plan_inexistente` | Rama `feat/002-x` sin plan | 3; stderr con `docs/plans/002-x.md` |
  | `session_tarea_inexistente` | `--task T9` | 3; `no existe la tarea T9` |
  | `session_tarea_no_confunde_prefijo` | El plan solo tiene `### T21 — y`; `--task T2` | 3 |
  | `session_tarea_formato` | `--task T2.1` y `--task t2` | 1 |
  | `session_agente_invalido` | `--agent planner` | 1 |
  | `session_agent_id_invalido` | `--agent-id "a b"` y un id de 65 caracteres | 1 |
  | `session_falta_argumento` | Sin `--agent-id` | 1 |
  | `session_reviewer_sin_veredicto` | `--agent reviewer` sin `--veredicto` | 1 |
  | `session_developer_con_veredicto` | `--agent developer --veredicto APPROVED` | 1 |
  | `session_entrada_invalida_no_ejecuta_gate` | stdin vacío; `runGate` espía | 2; sin fichero; `runGate` no se llama |
  | `session_agent_id_duplicado` | Ya hay una sesión con `agent_id: a1` en `short/` | 3 |
  | `cli_sin_argumentos` | `[]` | 1; stderr con `session` |
  | `cli_subcomando_desconocido` | `["borrar"]` | 1 |
  | `cli_ayuda` | `["--help"]` y `["help"]` | 0; stdout con `session`, `flush`, `close`, `promote`, `check`, `tags` |
  | `cli_proceso_real` | Repo temporal (`git init`, `git symbolic-ref HEAD refs/heads/chore/001-x`, plan `001-x.md` con `### T1 — a`, copia de `scripts/memory.mjs` y `scripts/lib/*.mjs`, y un `scripts/quality-gate.mjs` de prueba que imprime `──── Quality gate ────\n✔ A` y sale con 0); se ejecuta `node scripts/memory.mjs session --agent developer --agent-id a1 --task T1` con stdin `resumen: a` | 0; un único fichero con nombre `^\d{4}-\d{2}-\d{2}-001-x-T1-developer-1\.md$`, con `rama: chore/001-x` y `gate: verde` (de la fecha solo se comprueba el formato) |

### T4 — `flush`: volcado por tarea al medio plazo (developer)
- **Ficheros/módulos:** `scripts/lib/memory-plan.mjs`, `scripts/lib/memory-plan.test.mjs`, `scripts/memory.mjs`
- **Depende de:** T3
- **Cubre:** CA4
- **Subtareas:**
  - [ ] T4.1 `renderPlan(plan)` y `parsePlan(md)` para el formato **abierto**. La estructura es `{plan, estado, patterns?, tasks:[{task, cycles:[…], summaries:[…], decisions:[{slug,text,cycles}], errors:[{slug,text,cycles,issues}]}]}`.
  - [ ] T4.2 `mergeCycle(planDoc, task, devSessions, reviewerSession) → planDoc`. Añade el ciclo `k`; las entradas con un slug ya existente reciben el texto nuevo, se añade el ciclo y se unen los issues.
  - [ ] T4.3 `flush(root, plan, task, agentId)`, en este orden:
    1. lee el medio plazo si existe;
    2. sale con 3 si está `cerrado` o es ilegible;
    3. toma las sesiones de `(plan, task)`;
    4. sale con 3 si no hay exactamente una del reviewer o su `agent_id` no coincide;
    5. **si ese `agent_id` ya está en la columna Reviewer de la tarea**, no añade ciclo y solo borra las sesiones restantes (recuperación idempotente);
    6. si no, añade el ciclo y escribe el medio plazo (temporal + rename);
    7. borra las sesiones: primero las del developer y la del reviewer al final.
  - [ ] T4.4 Subcomando `flush --task Tn --agent-id <id>`. Escribe en stdout `docs/memory/medium/<plan>.md`, `ciclo: Tn C<k>` y `borradas: <m>`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `flush_crea_medio` | Sin medio plazo; sesiones developer T3 #1 y reviewer `r1` T3 `CHANGES_REQUESTED` | 0; el medio plazo coincide con el golden (ciclo 1 del ejemplo); `short/` vacío |
  | `flush_segundo_ciclo` | Lo anterior, más developer T3 #2 y reviewer `r2` `APPROVED` | Ciclo 2 añadido; igual al ejemplo completo de "Formatos" |
  | `flush_acumula_slug` | `error [validacion-args]` en C1 (`#12`) y en C2 (`#15`, texto nuevo) | Una sola entrada con el texto de C2 `(C1, C2 · #12, #15)` |
  | `flush_varios_developer` | 2 sesiones de developer y 1 de reviewer | `Developer` = 2; 3 líneas de resumen, las de developer en orden de `n` |
  | `flush_sin_developer` | Solo la sesión del reviewer | 0; `Developer` = 0 |
  | `flush_orden_tareas` | Se vuelca T10 y después T2 | Secciones `## T2` y `## T10`, en ese orden |
  | `flush_no_toca_otras_tareas` | Hay sesiones de T3 y de T4 | Vuelca y borra solo las de T3 |
  | `flush_sin_reviewer` | Solo sesiones de developer | 3; no escribe ni borra nada |
  | `flush_dos_reviewer` | 2 sesiones de reviewer para T3 | 3 |
  | `flush_agent_id_distinto` | Sesión del reviewer `r1` y se pide `--agent-id r9` | 3 |
  | `flush_plan_cerrado` | Medio plazo con `estado: cerrado` | 3 |
  | `flush_medio_ilegible` | Medio plazo sin front matter | 3; stderr con la ruta |
  | `flush_reintento_idempotente` | `r1` ya figura en el ciclo 1 y queda su sesión en `short/` | 0; no hay ciclo 2; la sesión se borra |
  | `flush_rama_sin_plan` | Rama `main` | 3 |
  | `flush_argumentos` | Sin `--task` o sin `--agent-id` | 1 |
  | `plan_roundtrip_abierto` | `parsePlan(renderPlan(doc))` | El mismo `doc` |
  | `plan_texto_con_parentesis` | Texto `usa (a) y (b)` | Se recupera literal |

### T5 — `close` (compactar) y `promote` (developer)
- **Ficheros/módulos:** `scripts/lib/memory-plan.mjs` (formato cerrado), `scripts/lib/memory-long.mjs`, `scripts/lib/memory-long.test.mjs`, `scripts/memory.mjs`
- **Depende de:** T4
- **Cubre:** CA5, CA6
- **Subtareas:**
  - [ ] T5.1 `renderPlan` y `parsePlan` admiten el formato **cerrado**. `compactPlan(doc) → doc` construye "Patrones del plan" y descarta los resúmenes y las entradas por tarea.
  - [ ] T5.2 `promote(root) → {written[], dropped[]}`, según D12, contando solo los planes cerrados.
  - [ ] T5.3 `close(root, plan)`:
    - sale con 3 si el medio plazo no existe o es ilegible;
    - si está abierto, exige el último ciclo de cada tarea en `APPROVED` y 0 sesiones del plan en `short/` (si no, sale con 3), y compacta;
    - si ya está cerrado, no lo modifica;
    - en ambos casos ejecuta `promote`.
  - [ ] T5.4 Subcomandos `close` y `promote` en la CLI.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `close_compacta` | El medio plazo abierto del ejemplo | Igual al golden del ejemplo cerrado |
  | `close_patrones_varias_tareas` | `decision [a]` en T1 ("v1") y T3 ("v3") | `- **a** — v3 (T1, T3)` |
  | `close_issues_varias_tareas` | `error [x]` en T2 (`#7`) y T4 (`#12`) | `(T2, T4 · #7, #12)` |
  | `close_tarea_sin_approved` | Último ciclo de T2 en `CHANGES_REQUESTED` | 3; nada cambia |
  | `close_sesiones_pendientes` | Una sesión de `001-memoria` en `short/` | 3; nada cambia |
  | `close_sin_medio` | No existe el medio plazo | 3 |
  | `close_idempotente` | Dos ejecuciones | La segunda sale con 0 y todo queda idéntico byte a byte |
  | `close_ejecuta_promote` | `close` válido | Existen `long/decisiones.md` y `long/errores.md` |
  | `plan_roundtrip_cerrado` | `parsePlan(renderPlan(doc cerrado))` | El mismo `doc` |
  | `promote_umbral_tres` | Slug `a` en 3 planes cerrados y `b` en 2 | Solo `a` |
  | `promote_ignora_abiertos` | `a` en 2 planes cerrados y 1 abierto | Sin entradas |
  | `promote_separa_tipos` | `x` como decisión en 3 planes y como error en 2 | Solo en `decisiones.md` |
  | `promote_texto_mas_reciente` | `a` en 001 ("v1"), 004 ("v4") y 002 ("v2") | `v4`; `(planes: 001-…, 002-…, 004-…)` |
  | `promote_vacio` | Sin `medium/` | 0; los dos ficheros vacíos (`- (ninguna)` / `- (ninguno)`), con 10 líneas cada uno |
  | `promote_formato_exacto` | 1 entrada | Igual al golden de "Formatos" |
  | `promote_limite_40` | 45 candidatos | 40 entradas; 5 líneas `descartado por límite` |
  | `promote_desempate` | Empate en nº de planes | Orden por plan más reciente desc. y después slug asc. |
  | `promote_tamano_total` | 40 decisiones y 40 errores | 49 líneas por fichero; suma ≤ 100 |
  | `promote_idempotente` | Dos ejecuciones | Ficheros idénticos |

### T6 — `check` y `tags` (developer)
- **Ficheros/módulos:** `scripts/lib/memory-long.mjs`, `scripts/lib/memory-check.test.mjs`, `scripts/memory.mjs`
- **Depende de:** T5
- **Cubre:** CA7, D13
- **Subtareas:**
  - [ ] T6.1 `check(root) → string[]` con las reglas de D13. No lee `short/` y normaliza `\r\n` antes de comparar. El estado del plan se detecta con `^- \*\*Estado:\*\* hecho\s*$`. El `.gitignore` se valida con la línea exacta `docs/memory/short/`.
  - [ ] T6.2 `tags(root) → string[]`.
  - [ ] T6.3 Subcomandos `check` (salida 4 si hay incidencias) y `tags`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `check_sin_memoria` | Raíz con un `.gitignore` correcto y sin `docs/memory/` | `[]`; la CLI sale con 0 |
  | `check_coherente` | Un plan cerrado, uno abierto y el largo plazo regenerado | `[]`; `Memoria OK (2 planes, 1 cerrados)` |
  | `check_ignora_short` | Lo anterior, más un fichero corrupto en `short/` | `[]` |
  | `check_no_canonico` | Una línea del medio plazo editada a mano (doble espacio) | 1 incidencia `formato no canónico`; la CLI sale con 4 |
  | `check_medio_ilegible` | Medio plazo sin front matter | 1 incidencia |
  | `check_nombre_incoherente` | `medium/002-x.md` con `plan: 001-memoria` | 1 incidencia |
  | `check_ciclos_saltados` | Ciclos 1 y 3 | 1 incidencia `ciclos no consecutivos` |
  | `check_cerrado_sin_approved` | Plan cerrado cuyo último ciclo de T2 es `CHANGES_REQUESTED` | 1 incidencia |
  | `check_largo_editado` | Una línea añadida a mano en `long/errores.md` | 1 incidencia con `ejecuta node scripts/memory.mjs promote` |
  | `check_plan_hecho_abierto` | `docs/plans/001-x.md` hecho y `medium/001-x.md` abierto | 1 incidencia con `ejecuta node scripts/memory.mjs close` |
  | `check_plan_hecho_sin_medio` | `000-governance` hecho, sin medio plazo | `[]` |
  | `check_gitignore` | `.gitignore` sin `docs/memory/short/` o sin `.gitignore` | 1 incidencia |
  | `check_tolera_crlf` | Memoria coherente convertida a CRLF | `[]` |
  | `tags_orden` | `decision a` en 2 planes, `decision b` en 2 y `error c` en 1 | `["decision a 2", "decision b 2", "error c 1"]` |
  | `tags_abiertos_y_cerrados` | `a` en un plan abierto (por tarea) y en otro cerrado (patrones) | `decision a 2` |
  | `tags_sin_memoria` | Raíz vacía | `[]`; sale con 0 |

### T7 — Política de memoria para los hooks (developer)
- **Ficheros/módulos:** `scripts/lib/memory-policy.mjs`, `scripts/lib/memory-policy.test.mjs`
- **Depende de:** T3 (`findSessionByAgentId`, `sessionsOfTask`), T4 (`parsePlan`), T6 (`tags`)
- **Cubre:** CA8, CA9, CA10 (lógica)
- **Subtareas:**
  - [ ] T7.1 `memoryCommandLine(command) → string | null`. Devuelve la primera línea si el comando es una invocación válida según T7.2 y `null` en otro caso.
  - [ ] T7.2 `checkMemoryToolUse(input, {root}) → {allow:true} | {allow:false, reason}`. Las reglas se aplican en este orden:
    1. **`Edit`, `Write` o `NotebookEdit` sobre una ruta bajo `docs/memory/`** (relativa a `root`, con `\` → `/` y sin distinguir mayúsculas): se deniega con `R11: la memoria solo se escribe con node scripts/memory.mjs`.
    2. **`Bash` o `PowerShell` con un comando que cumple `/\bnode\b[^\n]*(memory\.mjs|lib[\\/]memory-)/i`:** se exige la forma exacta.
       - La primera línea cumple `^node scripts/memory\.mjs (session|flush|close|promote|check|tags|help)((?: --[a-z][a-z-]* [A-Za-z0-9._-]+)*)( <<'EOF')?$`.
       - El heredoc solo se admite con `session`, y la **única** línea igual a `EOF` es la última.
       - Sin heredoc, el comando tiene una sola línea.
       
       Con la forma exacta cumplida, se aplica:
       - **`session`, `flush`, `close` o `promote` desde PowerShell:** se deniegan con `usa la herramienta Bash`.
       - **`check`, `tags` o `help`:** se permiten.
       - **`session`:** exige `agent_type` en {`developer`, `reviewer`}, `--agent` == `agent_type` y `--agent-id` == `agent_id`.
       - **`flush`:** exige `agent_type === "reviewer"` y `--agent-id` == `agent_id`.
       - **`close` y `promote`:** exigen `agent_type === "reviewer"`.
    3. **Cualquier otro comando que escribe y menciona `docs/memory`:** se deniega.
       - "Escribe" significa que cumple la regex de escritura de `protect-files` o `\bgit\s+(checkout|restore|rm|mv)\b`.
       - La mención se busca con `\` → `/` y sin distinguir mayúsculas.
    4. En cualquier otro caso, se permite.
  - [ ] T7.3 `subagentStartContext(input, {root}) → string | null`, con los textos de abajo.
  - [ ] T7.4 `subagentStopDecision(input, {root, branch}) → {block:false} | {block:true, reason}`, según D8.
- **Textos exactos:**
  - **`developer`:**
    ```
    [R11] agent_id de esta sesión: <id>
    Antes de terminar registra tu sesión con la herramienta Bash, con esta forma exacta:
    node scripts/memory.mjs session --agent developer --agent-id <id> --task <Tn> <<'EOF'
    resumen: <una línea>
    decision: [<slug>] <una línea>
    error: [<slug>] <una línea; enlaza #N si hay issue>
    EOF
    Slugs existentes (reutilízalos si encajan):
    <tags>
    ```
    `<tags>` son como máximo las 30 primeras líneas de `tags`, o `(ninguno)`.
  - **`reviewer`:** el mismo texto, con estos cambios:
    - en la orden: `--agent reviewer` y ` --veredicto <APPROVED|CHANGES_REQUESTED>` antes de ` <<'EOF'`;
    - justo después de la línea `EOF`, estas 4 líneas:
      ```
      Después vuelca la tarea, tanto con APPROVED como con CHANGES_REQUESTED:
      node scripts/memory.mjs flush --task <Tn> --agent-id <id>
      Si el orchestrator indica "última tarea: sí" y el veredicto es APPROVED, después:
      node scripts/memory.mjs close
      ```
  - **`planner`:** `[R11] Memoria a largo plazo (léela antes de planificar):`, una línea en blanco y el contenido de `long/decisiones.md` y `long/errores.md`, separados por una línea en blanco. Si no existe ninguno, `[R11] La memoria a largo plazo está vacía.`
  - **Cualquier otro agente:** `null`.
  - **Bloqueo por sesión ausente:** `falta tu entrada de sesión en docs/memory/short (R11). Ejecútala con la herramienta Bash antes de terminar:` + `\n` + las líneas de la orden `session` de su tipo, desde `node scripts/memory.mjs session …` hasta `EOF`.
  - **Bloqueo por volcado pendiente:** `quedan sesiones de <Tn> sin volcar en docs/memory/medium (R11). Ejecuta con la herramienta Bash antes de terminar:` + `\n` + `node scripts/memory.mjs flush --task <Tn> --agent-id <id>`.
- **Tests unitarios** (`root` temporal; las rutas se construyen con `path.join`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `politica_bloquea_write_memoria` | `Write` sobre `<root>/docs/memory/short/x.md`, como `developer` | Denegado; el motivo contiene `R11` y `scripts/memory.mjs` |
  | `politica_bloquea_edit_ruta_windows` | `Edit` sobre `<root>` + `\\DOCS\\Memory\\medium\\001-x.md` | Denegado |
  | `politica_bloquea_notebook` | `NotebookEdit` con `notebook_path` bajo `docs/memory/` | Denegado |
  | `politica_bloquea_orquestador` | `Edit` sobre la memoria sin `agent_type` | Denegado |
  | `politica_permite_fuera` | `Write` sobre `docs/plans/002-x.md` | Permitido |
  | `politica_no_confunde_prefijo` | `Write` sobre `docs/memoryx/a.md` | Permitido |
  | `politica_session_developer` | `Bash`, `developer`/`a1`: `node scripts/memory.mjs session --agent developer --agent-id a1 --task T2 <<'EOF'\nresumen: a -> b > c\nEOF` | Permitido |
  | `politica_session_reviewer` | `reviewer`/`r1`, con `--veredicto APPROVED` | Permitido |
  | `politica_session_agente_falso` | `developer` con `--agent reviewer` | Denegado |
  | `politica_session_id_falso` | `agent_id` `a1` con `--agent-id a2` | Denegado |
  | `politica_session_planner` | `planner` | Denegado |
  | `politica_session_orquestador` | Sin `agent_type` | Denegado |
  | `politica_flush_reviewer` | `node scripts/memory.mjs flush --task T2 --agent-id r1`, como `reviewer`/`r1` | Permitido |
  | `politica_flush_id_falso` | La misma orden con `agent_id` `r2` | Denegado |
  | `politica_flush_developer` | `flush`, como `developer` | Denegado |
  | `politica_close_reviewer` | `close`, como `reviewer` | Permitido |
  | `politica_close_developer` | `close`, como `developer` | Denegado |
  | `politica_promote_orquestador` | `promote` sin `agent_type` | Denegado |
  | `politica_lectura_cualquiera` | `check`, `tags` y `help` sin `agent_type` | Permitidos |
  | `politica_encadenado` | `node scripts/memory.mjs check && rm -rf docs/memory` | Denegado |
  | `politica_redireccion` | `node scripts/memory.mjs tags > docs/memory/long/x.md` | Denegado |
  | `politica_eof_intermedio` | Heredoc con `EOF` en la línea 2 y otra vez al final | Denegado |
  | `politica_heredoc_sin_cierre` | Heredoc sin línea `EOF` | Denegado |
  | `politica_heredoc_fuera_de_session` | `flush --task T2 --agent-id r1 <<'EOF'\nx\nEOF` | Denegado |
  | `politica_ruta_alternativa` | `node ./scripts/memory.mjs check` y `node scripts\\memory.mjs check` | Denegados |
  | `politica_import_directo` | `node -e "import('./scripts/lib/memory-plan.mjs')"` | Denegado |
  | `politica_argumento_entrecomillado` | `--task "T2"` | Denegado |
  | `politica_powershell_escritura` | `session` válido con `tool_name` `PowerShell` | Denegado; `usa la herramienta Bash` |
  | `politica_powershell_lectura` | `check` con `tool_name` `PowerShell` | Permitido |
  | `politica_bash_escribe_memoria` | `echo x > docs/memory/short/a.md` | Denegado |
  | `politica_bash_rm_windows` | `Remove-Item docs\\memory\\long\\errores.md` | Denegado |
  | `politica_bash_git_restore` | `git checkout -- docs/memory` y `git restore docs/memory/long` | Denegados |
  | `politica_bash_lee_memoria` | `cat docs/memory/long/errores.md` y `cat scripts/memory.mjs` | Permitidos |
  | `politica_bash_ajeno` | `cargo test` | Permitido |
  | `linea_comando_memoria` | El comando de `politica_session_developer` | Su primera línea |
  | `linea_comando_ajeno` | `git status` | `null` |
  | `inicio_developer` | `{agent_type:"developer", agent_id:"a1"}` | Contiene `agent_id de esta sesión: a1` y `--agent developer --agent-id a1 --task <Tn> <<'EOF'`; no contiene `flush` |
  | `inicio_reviewer` | `reviewer`/`r1` | Contiene `--veredicto <APPROVED|CHANGES_REQUESTED>`, `flush --task <Tn> --agent-id r1` y `node scripts/memory.mjs close` |
  | `inicio_incluye_tags` | Medio plazo con `decision a` | Contiene `decision a 1` |
  | `inicio_tags_vacios` | Sin memoria | Contiene `(ninguno)` |
  | `inicio_planner_con_largo` | `long/` con los 2 ficheros | Empieza por `[R11] Memoria a largo plazo` y contiene los 2 contenidos |
  | `inicio_planner_vacio` | Sin `long/` | `[R11] La memoria a largo plazo está vacía.` |
  | `inicio_otro_agente` | `Explore` | `null` |
  | `parada_developer_con_sesion` | Sesión `a1` en `short/`; rama `chore/001-memoria` | `{block:false}` |
  | `parada_developer_sin_sesion` | Sin sesión `a1` | Bloquea; el motivo contiene `--agent developer --agent-id a1` |
  | `parada_stop_hook_active` | El caso anterior con `stop_hook_active:true` | Bloquea |
  | `parada_reviewer_sin_sesion` | Sin sesión de `r1` y `r1` ausente del medio plazo | Bloquea con la orden `session --agent reviewer --agent-id r1` |
  | `parada_reviewer_sin_volcar` | Sesión de `r1` (T3) en `short/` | Bloquea; el motivo contiene `flush --task T3 --agent-id r1` |
  | `parada_reviewer_volcado` | `r1` en la columna Reviewer de `## T3` y ninguna sesión de T3 en `short/` | `{block:false}` |
  | `parada_reviewer_quedan_de_la_tarea` | `r1` volcado en T3, pero queda una sesión de developer de T3 | Bloquea con `flush --task T3 --agent-id r1` |
  | `parada_reviewer_otras_tareas_no_importan` | `r1` volcado en T3 y queda una sesión de T4 | `{block:false}` |
  | `parada_rama_sin_plan` | Rama `main` | `{block:false}` |
  | `parada_otro_agente` | `planner` | `{block:false}` |
  | `parada_id_exacto` | Solo existe `agent_id: a10`; se busca `a1` | Bloquea |
  | `parada_fichero_corrupto` | Un `.md` ilegible en `short/` y la sesión válida de `a1` | `{block:false}`, sin excepción |

### T8 — Agentes, CLAUDE.md y .gitignore (O)
- **Ficheros/módulos:** `.claude/agents/developer.md`, `.claude/agents/reviewer.md`, `.claude/agents/planner.md`, `CLAUDE.md`, `.gitignore`
- **Depende de:** T3–T6
- **Cubre:** CA14
- **Subtareas:**
  - [ ] T8.1 `.gitignore`: añadir la línea `docs/memory/short/`. Va antes de que se cree ninguna sesión real.
  - [ ] T8.2 `developer.md`:
    - En "Reglas", añadir `scripts/memory.mjs`, `scripts/lib/` y `docs/memory/` (este último, salvo vía `memory.mjs`) a la lista de ficheros que no edita.
    - El "Cierre obligatorio" queda así:
      1. Gate.
      2. **Nuevo:** "Registra tu sesión (R11). Es obligatorio: el hook `SubagentStop` no te deja terminar sin ella. Usa el `agent_id` de tu contexto inicial y la herramienta Bash, con la forma exacta…", seguido del bloque `session` de T7 para developer. Después: "Reutiliza los slugs existentes (`node scripts/memory.mjs tags`). Un error enlaza su issue (`#N`), no lo copia. Nunca incluyas secretos ni textos como `git commit` o `Co-Authored-By:`."
      3. Informe, con "Ruta de la entrada de sesión".
  - [ ] T8.3 `reviewer.md`:
    - "**Nunca modificas ficheros.**" pasa a "**Nunca modificas ficheros, salvo la memoria del equipo y solo a través de `node scripts/memory.mjs` (R11).**"
    - Nueva sección `## Memoria (R11)` con los pasos al terminar, siempre en este orden:
      1. `session` con `--agent reviewer` y `--veredicto`; los errores son los hallazgos, con su `#N`.
      2. `flush --task <Tn> --agent-id <id>`, con cualquier veredicto.
      3. Si el orchestrator indica `última tarea: sí` y el veredicto es `APPROVED`, `close`.
    - El bloque de veredicto añade `Memoria: docs/memory/medium/<plan>.md (Tn C<k>)` y, si se cerró, `Plan cerrado en memoria: sí`.
  - [ ] T8.4 `planner.md`: en "Antes de escribir", nuevo paso 0: "Lee siempre la memoria a largo plazo (`docs/memory/long/decisiones.md` y `errores.md`). El hook `SubagentStart` te la inyecta; si no aparece, léela. `docs/memory/medium/`, bajo demanda."
  - [ ] T8.5 `CLAUDE.md`:
    - Paso 6: "Revisión → `reviewer` con la ruta del plan, el rango del diff y `última tarea: sí|no`. El reviewer vuelca la memoria de la tarea en cada ciclo."
    - Paso 9: "Al acabar el plan, comprobar que `docs/memory/medium/NNN-slug.md` está en `estado: cerrado` y que `node scripts/memory.mjs check` está en verde antes de marcar `Estado: hecho`."
    - "Traspaso": añadir "si es la última tarea del plan".
    - Nueva sección `## Memoria del equipo (R11)`, con la frase "Solo se escribe con `node scripts/memory.mjs`; el orchestrator nunca la escribe" y esta tabla:

      | Nivel | Ruta | Versionado | Escribe | Lee siempre | Bajo demanda |
      |---|---|---|---|---|---|
      | Corto | `docs/memory/short/` | No (efímero hasta el volcado) | developer, reviewer | — | planner, orchestrator |
      | Medio | `docs/memory/medium/` | Sí | reviewer (`flush`, `close`) | — | developer, planner, orchestrator |
      | Largo | `docs/memory/long/` | Sí | reviewer (`promote`, `close`) | planner | developer, orchestrator |
- **Tests unitarios (comprobaciones):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `gitignore_short` | `Select-String .gitignore -Pattern '^docs/memory/short/$'` | 1 coincidencia |
  | `developer_cierre_memoria` | `Select-String .claude/agents/developer.md -Pattern 'memory.mjs session --agent developer'` | 1 coincidencia |
  | `reviewer_puede_memoria` | `Select-String .claude/agents/reviewer.md -Pattern 'salvo la memoria del equipo'` | 1 coincidencia |
  | `reviewer_flush_y_close` | Patrones `memory.mjs flush` y `memory.mjs close` | ≥ 1 coincidencia cada uno |
  | `reviewer_sin_write` | Línea `tools:` de `reviewer.md` | `tools: Read, Grep, Glob, Bash` |
  | `planner_lee_largo` | `Select-String .claude/agents/planner.md -Pattern 'docs/memory/long/'` | ≥ 1 coincidencia |
  | `claude_md_memoria` | `'^## Memoria del equipo \(R11\)$'` y `'última tarea'` | 1 coincidencia y ≥ 1 |

### T9 — Hooks: protección y memoria (U)
- **Ficheros/módulos:** `.claude/hooks/lib.mjs`, `.claude/hooks/protect-files.mjs`, `.claude/hooks/memory-gate.mjs` (nuevo)
- **Depende de:** T7 `APPROVED` y T1–T7 integradas. Después de T9 el developer ya no puede modificarlas.
- **Cubre:** CA8, CA9, CA10, CA15
- **Subtareas:**
  - [ ] T9.1 `lib.mjs`: en `PROTECTED_PATHS`, tras `/^scripts\/quality-gate\.mjs$/,`, añadir:
    ```js
      /^scripts\/memory(\.test)?\.mjs$/,
      /^scripts\/lib\//,
    ```
  - [ ] T9.2 Sustituir `protect-files.mjs` entero por:
    ```js
    // PreToolUse (Edit | Write | NotebookEdit | Bash | PowerShell):
    // impide modificar los ficheros de governance (R8) y escribir la memoria fuera de scripts/memory.mjs (R11).

    import { readHookInput, block, isProtected, PROTECTED_PATHS, PROJECT_DIR } from "./lib.mjs";

    const input = readHookInput();
    const toolInput = input.tool_input ?? {};
    // Import dinámico: si el módulo faltara, R8 sigue aplicándose.
    const policy = await import("../../scripts/lib/memory-policy.mjs").catch(() => null);

    const filePath = toolInput.file_path ?? toolInput.notebook_path;
    if (filePath && isProtected(filePath)) {
      block(`'${filePath}' es un fichero de governance y solo lo modifica el usuario. R8.`);
    }

    const rawCommand = toolInput.command;
    if (rawCommand) {
      // El cuerpo de un heredoc de memory.mjs son datos, no órdenes: R8 solo evalúa la línea de la orden.
      const command = policy?.memoryCommandLine(rawCommand) ?? rawCommand;
      const writes = /(>|\bsed\s+-i\b|\brm\b|\bmv\b|\bcp\b|\btee\b|Set-Content|Add-Content|Out-File|Remove-Item|Move-Item|Copy-Item|New-Item)/i;
      const normalized = command.replace(/\\/g, "/");
      const touchesProtected = PROTECTED_PATHS.some((re) =>
        new RegExp(re.source.replace(/^\^/, "(^|[\\s\"'/])"), "i").test(normalized),
      );
      if (writes.test(command) && touchesProtected) {
        block("el comando modifica ficheros de governance, que solo modifica el usuario. R8.");
      }
    }

    if (policy) {
      const memory = policy.checkMemoryToolUse(input, { root: PROJECT_DIR });
      if (!memory.allow) block(memory.reason);
    }

    process.exit(0);
    ```
  - [ ] T9.3 Crear `memory-gate.mjs`:
    ```js
    // SubagentStart / SubagentStop: memoria del equipo. R11.
    // Start: inyecta el agent_id y las órdenes de memoria (developer, reviewer) o el largo plazo (planner).
    // Stop: no deja terminar a developer sin su sesión ni a reviewer sin su sesión volcada.

    import { readHookInput, block, git, PROJECT_DIR } from "./lib.mjs";
    import { subagentStartContext, subagentStopDecision } from "../../scripts/lib/memory-policy.mjs";

    const input = readHookInput();

    if (input.hook_event_name === "SubagentStart") {
      const context = subagentStartContext(input, { root: PROJECT_DIR });
      if (context) {
        process.stdout.write(
          JSON.stringify({ hookSpecificOutput: { hookEventName: "SubagentStart", additionalContext: context } }),
        );
      }
      process.exit(0);
    }

    if (input.hook_event_name === "SubagentStop") {
      const decision = subagentStopDecision(input, { root: PROJECT_DIR, branch: git(["branch", "--show-current"]) });
      if (decision.block) block(decision.reason);
    }

    process.exit(0);
    ```
  - [ ] T9.4 Guardar los tres ficheros con LF.
- **Tests unitarios:** ver T13.1.

### T10 — Registrar hooks y permisos en settings (U)
- **Ficheros/módulos:** `.claude/settings.json`
- **Depende de:** T9
- **Cubre:** CA9, CA10
- **Subtareas:**
  - [ ] T10.1 En `permissions.allow`, tras `"Bash(node scripts/quality-gate.mjs)",`, añadir:
    ```json
          "Bash(node scripts/memory.mjs:*)",
          "Bash(node --test:*)",
    ```
  - [ ] T10.2 En `hooks`, tras el bloque `"Stop"` (con su coma), añadir:
    ```json
        "SubagentStart": [
          {
            "matcher": "planner|developer|reviewer",
            "hooks": [
              { "type": "command", "command": "node \"$CLAUDE_PROJECT_DIR/.claude/hooks/memory-gate.mjs\"" }
            ]
          }
        ],
        "SubagentStop": [
          {
            "matcher": "developer|reviewer",
            "hooks": [
              { "type": "command", "command": "node \"$CLAUDE_PROJECT_DIR/.claude/hooks/memory-gate.mjs\"" }
            ]
          }
        ]
    ```
  - [ ] T10.3 Validar con `node -e "JSON.parse(require('fs').readFileSync('.claude/settings.json','utf8'))"`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `settings_json_valido` | T10.3 | Código 0 |
  | `settings_matchers` | Imprimir `hooks.SubagentStart[0].matcher` y `hooks.SubagentStop[0].matcher` | `planner|developer|reviewer` y `developer|reviewer` |

### T11 — Quality gate: secretos, tests de scripts y check (U)
- **Ficheros/módulos:** `scripts/quality-gate.mjs`
- **Depende de:** T1, T6 y T8.1, que deben estar integradas: con el `.gitignore` sin cambiar, `check` falla.
- **Cubre:** CA11, CA12
- **Subtareas:**
  - [ ] T11.0 Comprobar `node --version` ≥ 22 en local.
  - [ ] T11.1 Tras los imports, añadir `import { findSecrets } from "./lib/secrets.mjs";`.
  - [ ] T11.2 Tras `SKIP_DIRS`, añadir:
    ```js
    const SECRET_DIRS = [...SOURCE_DIRS, "scripts", "docs/memory"];
    const SECRET_EXTS = new Set([...SOURCE_EXTS, ".md", ".json", ".toml", ".yml", ".yaml"]);
    ```
  - [ ] T11.3 Sustituir `walk` por:
    ```js
    function walk(dir, out, exts = SOURCE_EXTS) {
      if (!existsSync(dir)) return out;
      for (const name of readdirSync(dir)) {
        if (SKIP_DIRS.has(name)) continue;
        const full = join(dir, name);
        if (statSync(full).isDirectory()) walk(full, out, exts);
        else if (exts.has(extname(name))) out.push(full);
      }
      return out;
    }
    ```
  - [ ] T11.4 Tras `scanForbiddenPatterns`, añadir:
    ```js
    function scanSecrets() {
      process.stdout.write("\n▶ Secretos (governance R6)\n");
      const violations = [];
      for (const dir of SECRET_DIRS) {
        for (const file of walk(join(ROOT, dir), [], SECRET_EXTS)) {
          for (const { line, kind } of findSecrets(readFileSync(file, "utf8"))) {
            violations.push(`  ${relative(ROOT, file)}:${line} — posible secreto (${kind})`);
          }
        }
      }
      violations.forEach((v) => process.stdout.write(v + "\n"));
      return violations.length === 0;
    }
    ```
  - [ ] T11.5 En `buildSteps`, sustituir `steps.push(["Patrones prohibidos", scanForbiddenPatterns]);` por:
    ```js
      if (existsSync(join(ROOT, "scripts", "memory.mjs"))) {
        steps.push(["Tests scripts", () => run("Tests scripts", 'node --test "scripts/**/*.test.mjs"')]);
        steps.push(["Memoria (R11)", () => run("Memoria (R11)", "node scripts/memory.mjs check")]);
      }
      steps.push(["Patrones prohibidos", scanForbiddenPatterns]);
      steps.push(["Secretos", scanSecrets]);
    ```
- **Tests unitarios** (en la terminal del usuario, sobre una copia del repo sin `.git` en `$env:TEMP\gate-r11`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `gate_verde_con_memoria` | El gate en el repo real | 0; `✔ Tests scripts`, `✔ Memoria (R11)`, `✔ Secretos`, `✔ Patrones prohibidos` |
  | `gate_secreto_en_short` | En la copia, `docs/memory/short/x.md` con `hf_` + 34 `A` | 1; `docs/memory/short/x.md:1 — posible secreto (hugging-face)` (`Memoria (R11)` sigue en verde: no lee `short/`) |
  | `gate_secreto_en_scripts` | En la copia, `scripts/tmp.mjs` con una clave AWS literal completa | 1; `posible secreto (aws)` |
  | `gate_test_rojo` | En la copia, `scripts/lib/falla.test.mjs` con `assert.equal(1, 2)` | 1; `✖ Tests scripts` |
  | `gate_check_rojo` | En la copia, `docs/memory/short/` quitado de `.gitignore` | 1; `✖ Memoria (R11)` |
  | `gate_sin_memory_mjs` | En la copia, borrar `scripts/memory.mjs` | No aparecen `Tests scripts` ni `Memoria (R11)`; `Secretos` sí |

### T12 — GOVERNANCE: R11, R8, §1, §2 y DoD (U)
- **Ficheros/módulos:** `docs/GOVERNANCE.md`
- **Depende de:** — (se aplica junto a T9–T11)
- **Cubre:** CA13
- **Subtareas:**
  - [ ] T12.1 §1, fila "Hooks de Claude Code": añadir a "Qué bloquea" `, escribir la memoria fuera de scripts/memory.mjs, que developer/reviewer terminen sin registrar o volcar su sesión`.
  - [ ] T12.2 §2: antes de "Patrones prohibidos", insertar:
    ```
    | Tests de scripts | `node --test "scripts/**/*.test.mjs"` | `scripts/memory.mjs` |
    | Memoria (R11) | `node scripts/memory.mjs check` | `scripts/memory.mjs` |
    ```
    y después: `| Secretos (R6) | escaneo propio de código, scripts/ y docs/memory/ | siempre |`.
  - [ ] T12.3 Lista de R8: `.claude/settings.json`, `.claude/hooks/**`, `.githooks/**`, `scripts/quality-gate.mjs`, `scripts/memory.mjs`, `scripts/memory.test.mjs`, `scripts/lib/**`, `.github/workflows/**`, `docs/GOVERNANCE.md`.
  - [ ] T12.4 Tras R10, añadir:
    ```
    ### R11 — Memoria del equipo
    - Tres niveles en `docs/memory/`: `short/` (una entrada efímera por sesión de `developer` o `reviewer`, no versionada), `medium/NNN-slug.md` (memoria del plan: una sección por tarea con sus ciclos de revisión; al cerrar el plan se compacta en "Patrones del plan") y `long/decisiones.md` + `long/errores.md` (patrones presentes en 3 planes cerrados o más; 98 líneas como máximo en total).
    - La memoria solo se escribe con `node scripts/memory.mjs`, desde la herramienta Bash y con la forma exacta de comando. Nadie la edita a mano salvo el usuario. `session`: `developer` y `reviewer`, cada uno con su propia identidad. `flush`, `close` y `promote`: `reviewer`. El orchestrator y el planner solo la leen; el planner lee siempre `long/`.
    - Un `developer` no termina sin su entrada de sesión. Un `reviewer` no termina sin la suya ni sin volcar (`flush`) las sesiones de la tarea revisada en el medio plazo, sea cual sea el veredicto (hook `SubagentStop`). Los campos deterministas (fecha, rama, plan, tarea, ficheros cambiados, resultado del gate) los rellena el script, nunca el agente.
    - Con `APPROVED` en la última tarea, el reviewer cierra la memoria del plan (`close`). Un plan con `Estado: hecho` no puede tener su memoria abierta (`memory.mjs check`, en el gate).
    - Un error de memoria enlaza al issue (`#N`) y no lo copia: una sola línea de 200 caracteres como máximo.
    - Sin secretos en la memoria (R6): `memory.mjs` rechaza la entrada y el gate escanea `docs/memory/`.
    ```
  - [ ] T12.5 En §4, **antes** de la última línea (la de R9), insertar: `- [ ] Sesiones registradas y volcadas en la memoria del plan; si es la última tarea, memoria cerrada (R11).`
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `governance_tiene_r11` | `Select-String docs/GOVERNANCE.md -Pattern '^### R11 — Memoria del equipo$'` | 1 coincidencia |
  | `governance_orden_r10_r11` | Nº de línea de `### R10` y de `### R11` | R10 < R11 |
  | `governance_r8_memoria` | `'scripts/lib/\*\*'` | ≥ 1 coincidencia dentro de R8 |
  | `governance_gate_pasos` | `'^\| (Tests de scripts|Memoria \(R11\)|Secretos \(R6\)) \|'` | 3 coincidencias |
  | `governance_dod_r9_sigue_ultima` | Última línea `- [ ]` de §4 | La de R9 (CA8 del plan 000) |
  | `governance_dod_r11` | `'memoria cerrada \(R11\)'` | 1 coincidencia |

### T13 — Verificación de los hooks (U en terminal + O en vivo)
- **Ficheros/módulos:** `.claude/hooks/protect-files.mjs`, `.claude/hooks/memory-gate.mjs`, `.claude/settings.json`
- **Depende de:** T9, T10, T11
- **Cubre:** CA8, CA9, CA10, CA15, CA16 (identidad del agente)
- **T13.1 (U, terminal, determinista y sin red):**
  - Preparar un repo temporal `$env:TEMP\r11-repo`:
    - `git init` y `git symbolic-ref HEAD refs/heads/chore/001-memoria`;
    - `docs/plans/001-memoria.md` con `### T13 — x`;
    - un `.gitignore` con `docs/memory/short/`;
    - una copia de `scripts/`.
  - Ejecutar cada caso con `$env:CLAUDE_PROJECT_DIR="$env:TEMP\r11-repo"; Get-Content -Encoding utf8 case.json | node W:\app-llama\.claude\hooks\<hook>.mjs`.

  | Test | Hook y `case.json` | Resultado esperado |
  |---|---|---|
  | `hook_write_memoria_bloqueado` | protect-files, `{"tool_name":"Write","agent_type":"developer","agent_id":"a1","tool_input":{"file_path":"<repo>\\docs\\memory\\medium\\001-memoria.md"}}` | 2, `R11` |
  | `hook_orquestador_close` | protect-files, `{"tool_name":"Bash","tool_input":{"command":"node scripts/memory.mjs close"}}` | 2 |
  | `hook_reviewer_flush` | protect-files, `"agent_type":"reviewer","agent_id":"r1"`, `node scripts/memory.mjs flush --task T13 --agent-id r1` | 0 |
  | `hook_flush_id_falso` | El mismo caso con `--agent-id r2` | 2 |
  | `hook_session_identidad_falsa` | `developer`/`a1` con `session --agent developer --agent-id zz …` | 2 |
  | `hook_heredoc_con_mayor` | `session` válido con el cuerpo `resumen: a -> b` | 0 |
  | `hook_r8_memory_mjs` | `Edit` de `<repo>/scripts/memory.mjs` | 2, `R8` |
  | `hook_r8_lib` | `Edit` de `scripts/lib/memory-policy.mjs` | 2, `R8` |
  | `hook_r8_sin_policy` | Con `scripts/lib/memory-policy.mjs` renombrado en la copia, `Edit` de `.claude/settings.json` | 2, `R8` |
  | `hook_stop_developer_sin_sesion` | memory-gate, `{"hook_event_name":"SubagentStop","agent_type":"developer","agent_id":"zz","stop_hook_active":false}` | 2; `--agent developer --agent-id zz` |
  | `hook_stop_developer_con_sesion` | El mismo caso, tras crear la sesión `zz` de T13 con `memory.mjs session` en la copia | 0 |
  | `hook_stop_reviewer_sin_volcar` | Sesión del reviewer `rr` (T13) creada; SubagentStop `reviewer`/`rr` | 2; `flush --task T13 --agent-id rr` |
  | `hook_stop_reviewer_volcado` | Tras `node scripts/memory.mjs flush --task T13 --agent-id rr` en la copia | 0 |
  | `hook_stop_planner` | SubagentStop `planner` | 0 |
  | `hook_start_developer` | `{"hook_event_name":"SubagentStart","agent_type":"developer","agent_id":"a1"}` | 0; JSON con `hookSpecificOutput.hookEventName == "SubagentStart"` y `additionalContext` que contiene `a1` |
  | `hook_start_reviewer` | SubagentStart `reviewer`/`r1` | `additionalContext` contiene `flush --task <Tn> --agent-id r1` |
  | `hook_start_planner_vacio` | SubagentStart `planner` | `additionalContext` == `[R11] La memoria a largo plazo está vacía.` |
  | `hook_start_explore` | SubagentStart `Explore` | 0; stdout vacío |
- **T13.2 (O, en vivo en `chore/001-memoria`, tras reiniciar Claude Code para que cargue el settings):**
  - [ ] a) El orchestrator ejecuta `node scripts/memory.mjs close`. Esperado: bloqueado por `R11`, lo que confirma que la sesión principal no tiene `agent_type`.
  - [ ] b) El orchestrator ejecuta `node scripts/memory.mjs check`. Esperado: permitido.
  - [ ] c) Lanza el `developer` con este encargo: "Prueba de humo de memoria, tarea T13 del plan 001. No cambies código. 1) Intenta `Write` de `docs/memory/short/prueba.md`. 2) Intenta `session` con `--agent reviewer`. 3) Intenta terminar sin sesión. 4) Registra la sesión con tu `agent_id`, resumen `prueba de humo de memoria`. Informa del resultado literal de cada paso." Esperado:
    - 1) y 2) bloqueados por `R11`;
    - 3) bloqueado por `SubagentStop`, con su `agent_id`;
    - 4) crea `short/<fecha>-001-memoria-T13-developer-1.md`, con el mismo `agent_id` que se inyectó.
  - [ ] d) Lanza el `reviewer` sobre T13 (resultados de T13.1 y T13.2.a–c) con `última tarea: no`, y le pide que intente terminar tras `session` y antes de `flush`. Esperado:
    - bloqueado con la orden `flush`;
    - después de volcar, termina;
    - `medium/001-memoria.md` tiene `## T13` con `C1`, y no quedan sesiones de T13 en `short/`.
  - [ ] e) Lanza el `planner` con este encargo: "Responde solo si tu contexto inicial contiene `[R11] Memoria a largo plazo` o `[R11] La memoria a largo plazo está vacía.`, y cita la línea." Esperado: cita la línea.
  - [ ] f) Si a) se permite o c.1) no se bloquea, hay que parar y escalar al usuario (D6).

### T14 — Revisión final, cierre de memoria y cierre del plan (O + reviewer)
- **Ficheros/módulos:** el diff de la rama, `docs/memory/` y este plan
- **Depende de:** T1–T13
- **Cubre:** CA16
- **Subtareas:**
  - [ ] T14.1 El orchestrator lanza el `reviewer` con este plan, el diff de T8–T13 (incluidos los ficheros U), los resultados de T11–T13 y `última tarea: sí`.
  - [ ] T14.2 El reviewer ejecuta `session` y `flush --task T14`. Si el veredicto es `APPROVED`, ejecuta también `close`. Con `CHANGES_REQUESTED`, el usuario aplica los cambios en los ficheros protegidos (R8); como máximo, 3 ciclos.
  - [ ] T14.3 El orchestrator marca las tareas y pone `Estado: hecho`. Después ejecuta `check` y el gate, y hace el commit con R9.
- **Tests unitarios (comprobaciones):**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cierre_medio_cerrado` | `Select-String docs/memory/medium/001-memoria.md -Pattern '^estado: cerrado$'` | 1 coincidencia |
  | `cierre_tareas_volcadas` | `Select-String … -Pattern '^## T1[34]$'` | 2 coincidencias |
  | `cierre_short_no_versionado` | `git ls-files docs/memory/short` | Vacío |
  | `cierre_sin_pendientes` | `Get-ChildItem docs/memory/short/*-001-memoria-*.md` | Vacío |
  | `cierre_check_verde` | `node scripts/memory.mjs check` | 0 |
  | `cierre_largo_tamano` | `(Get-Content docs/memory/long/*.md).Count` | ≤ 100 |

## Fuera de alcance

- Deduplicación semántica de slugs: `tags` y la inyección de `SubagentStart` lo mitigan.
- Comprobar con `gh` que los `#N` existen, porque requiere red.
- Sesiones del planner y del orchestrator.
- Volcar en el medio plazo las sesiones de developer que nunca llegan a revisarse (por ejemplo, si se abandona una tarea). Se quedan en `short/` local y `close` se niega mientras existan.
- Ampliar R2 a `scripts/`.
- Migrar el plan 000 a la memoria.
- Escanear secretos en `docs/plans/`.
- La protección de rama de `main`.
- Optimizar el coste del gate en `session` (aceptado; irá en otro plan cuando entre Rust).

## Riesgos

| Riesgo | Mitigación |
|---|---|
| `agent_type` o `agent_id` no llegan a PreToolUse, o el `agent_id` cambia entre eventos. | T13.2 lo verifica en vivo. Si falla, las escrituras quedan bloqueadas para todos y se aplica la alternativa de D6. |
| Las sesiones efímeras se pierden si se borra `short/` o se cambia de máquina antes del volcado. | El volcado ocurre en cada revisión, minutos después de la sesión. Lo perdido es solo el detalle de ese ciclo. `close` exige que no queden sesiones del plan. |
| Un fallo entre la escritura del medio plazo y el borrado de las sesiones. | La escritura es atómica (fichero temporal y `rename`). Si se reintenta el volcado, detecta el `agent_id` del reviewer ya volcado y solo borra (test `flush_reintento_idempotente`). |
| Los slugs divergen y nada llega al largo plazo. | Se inyectan los slugs existentes y `tags`. El reviewer puede pedir un renombrado como hallazgo `minor`. |
| `session` vuelve a ejecutar el gate, lo que duplica su coste. | Aceptado por el usuario. Se optimizará en otro plan. |
| El tope de 8 continuaciones deja terminar a un agente sin sesión o sin volcado. | El informe incluye la sesión o `Memoria: … (Tn C<k>)`, y el orchestrator lo comprueba. `close` falla si quedan sesiones. |
| Con `core.autocrlf=true` en Windows, los ficheros generados se extraen con CRLF. | `check` y los parsers normalizan `\r\n` (`check_tolera_crlf`). |
| El cuerpo del heredoc con `git commit` o `Co-Authored-By:` activa guard-shell. | Bloquea de más, nunca de menos. Está documentado en el cierre de los agentes. |
| La regla `allow` con heredoc multilínea puede pedir permiso. | El usuario lo aprueba; se ajusta en otro plan si molesta. |
| Un comando ofuscado (`sh -c "no""de …"`) burla la detección. | Es la misma limitación que R8. `check` en el gate y en la CI valida todo lo versionado. |
| `node --test` con globs requiere Node ≥ 22. | T11.0. La CI usa Node 24. |
| Los tokens de ejemplo de los tests se detectan en el paso de Secretos. | Se construyen concatenando (`secrets_no_se_detecta_a_si_mismo`). |
| Antes de T9 y T10 las sesiones no son obligatorias. | Se acepta. El dogfooding empieza en T13. |

## Preguntas abiertas

- Ninguna. P1 (re-ejecutar el gate en `session`) y P2 (40 entradas por fichero, 98 líneas y umbral de 3 planes) las ha respondido el usuario y figuran en D11 y D12.
