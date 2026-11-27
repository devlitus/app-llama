---
schema: memoria-plan/2
plan: 011-aserto-rechazo-api
estado: cerrado
---

# Memoria del plan 011-aserto-rechazo-api

## Patrones del plan

### Decisiones

- **aserto-por-identidad** — rejects.toBe(ipcError) en vez de toThrow, que compara por subcadena (T1)
- **refactor-equivalente** — cambio limitado al test, api.ts intacto; la identidad del error es el contrato de propagación (T1)

### Errores

- **aserto-debil** — rejects.toBeDefined() pasaba con M1 (otro error) y sin mock; corregido (T1)

## T1

### Ciclos

| Ciclo | Fecha | Developer | Reviewer | Gate | Veredicto |
|---|---|---|---|---|---|
| 1 | 2026-11-27 | 1 | a3ffff87088cc9e46 | verde | APPROVED |
