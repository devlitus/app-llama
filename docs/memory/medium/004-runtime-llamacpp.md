---
schema: memoria-plan/2
plan: 004-runtime-llamacpp
estado: abierto
---

# Memoria del plan 004-runtime-llamacpp

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a7c554070d131c111 | verde | APPROVED |

### Resúmenes

- C1 · developer · T1 plan 004: modelo Release/Asset, parse_releases, build_number, sha256, latest_build_release y RuntimeError inicial, con fixtures.
- C1 · reviewer · T1 aprobada; gate verde, CA4 y los 9 tests del plan presentes; solo hallazgos minor

### Decisiones

- **refactor-equivalente** — se acepta decodificar hex a mano sin el crate hex hasta T5 y mod release privado con re-exports (C1)

### Errores

- **validacion-hex-laxa** — decode_sha256_hex acepta '+' porque u8::from_str_radix admite signo; minor, falla en lado seguro (C1)
