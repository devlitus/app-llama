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
