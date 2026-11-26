---
schema: memoria-plan/2
plan: 002-falsos-positivos-hooks
estado: abierto
---

# Memoria del plan 002-falsos-positivos-hooks

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ae406e9e243cbf440 | verde | APPROVED |

### Resúmenes

- C1 · developer · T1 retirar R8: protect-files solo R11 con safely, lib sin PROTECTED_PATHS, memoryCommandLine eliminada, governance y developer.md actualizados, e2e nuevo.
- C1 · reviewer · T1 retirar R8 revisada; gate verde, 7 e2e pasan, CA4/CA10/CA11 cubiertos; solo hallazgos minor.

### Decisiones

- **limitacion-documentada** — La linea 68 de GOVERNANCE en el rg de CA11 es defecto del plan (T1.5 manda ese texto), no de la implementacion; no bloquea. (C1)

### Errores

- **reserva-sin-test** — La reserva fail-closed de protect-files sin memory-policy no tiene test (minor, el plan no lo exige). (C1)

## T2

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a04e0f2e70405583f | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a66e47dab2274d979 | verde | APPROVED |

### Resúmenes

- C1 · developer · T2 analizador léxico de Bash (parseCommandList) con 9 tests, gate verde
- C1 · reviewer · shell-words cumple los 9 tests y el gate, pero diverge de Bash en comentarios y comodines y eso oculta pushes a T4
- C2 · developer · T2 ciclo 2: analizador devuelve null ante comentarios y comodines sin comillas; &> sin prefijo de descriptor
- C2 · reviewer · ciclo 2 de T2; #15 y #16 resueltos y con test, sin regresiones; gate verde

### Decisiones

- **analizador-null-ante-duda** — el # al inicio de palabra (tras separador, redirección o continuación) y los comodines sin comillas, también en operandos de redirección, devuelven null (C1, C2)
- **limitacion-documentada** — parseCommandList devuelve null ante $, comillas invertidas, paréntesis, }, \r, << y comillas escapadas; {} solo como literal (C1)

### Errores

- **comentario-oculta-orden** — un comentario con comillas escondía una orden; corregido, issue #15 (C1, C2 · #15)
- **glob-sin-comillas** — comodines sin comillas llegaban literales; corregido, issue #16 (C1, C2 · #16)

## T3

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | a3207262c8c2205fa | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a1ba82b14b87c5611 | verde | APPROVED |

### Resúmenes

- C1 · developer · T3 stripDataHeredocs en shell-heredoc.mjs con tests normativos y de cabeceras sospechosas.
- C1 · reviewer · T3 stripDataHeredocs revisado; gate verde pero despoja el cuerpo de un heredoc canalizado con | al final de la cabecera.
- C2 · developer · Ciclo 2 de T3: stripDataHeredocs deja intacta la orden si tras la cabecera hay una tubería (#17).
- C2 · reviewer · Ciclo 2 de T3: #17 resuelto (| , ||, |&, | \, && y & tras la cabecera dejan la orden intacta); gate verde, 6/6 tests.

### Decisiones

- **analizador-null-ante-duda** — Rechazar cualquier | en el resto de la cabecera, incluso entre comillas, es conservador e inocuo. (C1, C2)
- **limitacion-documentada** — hash -p /bin/sh cat o alias cat=sh con expand_aliases en lineas previas hacen que cat ejecute el cuerpo despojado; evasion deliberada de la misma clase que base64 | sh, minor sin issue. (C2)

### Errores

- **tuberia-tras-cabecera** — Heredoc con cabecera terminada en | (también |& y 2>&1 |) se despojaba y ocultaba un cuerpo ejecutable; #17. (C1, C2 · #17)

## T4

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ac66b75c86e420c2f | verde | CHANGES_REQUESTED |
| 2 | 2026-11-26 | 1 | a0b0931e5e80d3d1f | verde | CHANGES_REQUESTED |
| 3 | 2026-11-26 | 1 | ac93d0e14be38532d | verde | CHANGES_REQUESTED |
| 4 | 2026-11-26 | 1 | a4921988fa5f95ab8 | verde | CHANGES_REQUESTED |

### Resúmenes

- C1 · developer · T4 R5 analizado: push-guard.mjs (checkPush, legacyPushVerdict), guard-shell.mjs con import dinamico y try/catch, 14 tests unitarios y 5 e2e.
- C1 · reviewer · checkPush cubre los CA de la tabla pero deja 4 familias de evasiones reales de R5; gate verde
- C2 · developer · T4 ciclo 2: abreviaturas, DWIM de refs, orden anidada o con xargs, config en linea y reserva testeable (pushVerdict extraido); gate en verde
- C2 · reviewer · T4 ciclo 2; #18 y #19 resueltos; #20 y #21 parciales, con regresiones frente a la regla anterior; gate verde
- C3 · developer · T4 ciclo 3: red de seguridad legacy en palabras anidadas, hidden heredado, forma git-push, config en linea con built-in que ejecuta git, import dinamico de push-hook
- C3 · reviewer · ciclo 3 de T4; #18-#21 resueltos y gate verde, pero hay 2 regresiones nuevas frente a legacyPushVerdict y un falso positivo nuevo con -c
- C4 · developer · T4 ciclo 4: #22 (redirecciones y alias) y #23 (-c sin push) corregidos, mas quitar barras en isNestedPush y en la reserva.
- C4 · reviewer · ciclo 4 de T4; cierra here-string y #23, pero quedan alias con prefijo o con barra invertida y --config-env relajado

### Decisiones

- **analizador-null-ante-duda** — alias/unalias que menciona git vuelve opaca la orden completa; el destino de redireccion pasa por checkNestedWord. (C1, C2, C3, C4)
- **limitacion-documentada** — bash < f y alias con comillas vacías aceptados como deuda, porque la regla anterior tampoco los bloqueaba (C1, C3, C4)

### Errores

- **config-en-linea** — la relajación de -c alcanza a --config-env, cuyo valor viene del entorno y nunca se menciona (#24) (C1, C2, C3, C4 · #21, #22, #23, #24)
- **opcion-abreviada** — prefijos de --all/--mirror/--branches/--for* tratados como la opcion (#18); rojo previo: 4 tests de push-guard fallaban y push-hook.test no cargaba (C1, C2 · #18)
- **orden-anidada** — definesGitAlias solo mira words[0] y mentionsGit no quita la barra invertida; command/builtin/X=1/!/if delante de alias se permiten (#22) (C1, C2, C3, C4 · #20, #21, #22)
- **refspec-dwim** — heads/<rama> y remotes/o/<rama> normalizados como refs/heads (#19) (C1, C2 · #19)
- **reserva-sin-test** — pushVerdict en scripts/lib/push-hook.mjs con tests de excepcion y reserva (C1, C2)

## T5

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-26 | 1 | ad93d98d9bf1d2ac8 | verde | APPROVED |

### Resúmenes

- C1 · developer · T5 hecha: R11 admite listas de lecturas, cd/pwd, check/tags/help y despoja heredocs de datos; gate verde (450 tests). Rojo previo: 3 e2e fallaban y el test de lib no cargaba (sin splitCommandList).
- C1 · reviewer · T5 aprobada; sondeadas 45 variantes de escritura en memoria (redirecciones, cd, heredocs, node check, PowerShell) sin ninguna nueva escritura directa que pase

### Decisiones

- **analizador-null-ante-duda** — splitCommandList devuelve null con comillas escapadas o sin cerrar y la lista blanca sigue cerrada. (C1)
- **limitacion-documentada** — cd persistente entre llamadas y cd+node scripts/memory.mjs check ejecutan codigo o rutas relativas fuera del alcance de R11, ya posible antes con cd docs o node fichero (C1)

### Errores

- **orden-anidada** — un heredoc cat a fichero seguido de sh fichero en la misma orden pasa tras el despojo; antes se negaba, aunque Write+sh ya lo permitia (C1)
