---
schema: memoria-larga/1
tipo: decisiones
---

# Decisiones recurrentes

<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->

- **limitacion-documentada** — la verificación manual no registró hash, modelo dividido ni repo gated; la anotación lo declara explícitamente (planes: 001-memoria, 002-falsos-positivos-hooks, 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas)
- **refactor-equivalente** — cambio limitado al test, api.ts intacto; la identidad del error es el contrato de propagación (planes: 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas, 011-aserto-rechazo-api)
- **aserto-por-identidad** — rejects.toBe(ipcError) en vez de toThrow, que compara por subcadena (planes: 005-explorador-hf, 006-gestor-descargas, 011-aserto-rechazo-api)
- **analizador-null-ante-duda** — 206 con total * o cabecera inválida da UnexpectedContentRange; aceptado porque T6 reinicia desde 0 una vez (planes: 002-falsos-positivos-hooks, 004-runtime-llamacpp, 006-gestor-descargas)
- **csp-estricta** — los dos progress salen sin style y la barra principal solo lee progress.download (CA13 cubierto) (planes: 003-esqueleto, 004-runtime-llamacpp, 006-gestor-descargas)
