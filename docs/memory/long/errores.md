---
schema: memoria-larga/1
tipo: errores
---

# Errores recurrentes

<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->

- **aserto-debil** — rejects.toBeDefined() pasaba con M1 (otro error) y sin mock; corregido (planes: 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas, 011-aserto-rechazo-api)
- **reserva-sin-test** — #87 remove en Cancelled borraba el final de una descarga posterior de la misma variante; corregido con PartsOnly. (planes: 002-falsos-positivos-hooks, 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas)
- **codigo-muerto** — minor: force siempre devuelve true y finish ignora el valor; el plan sigue diciendo force() sin argumentos (planes: 003-esqueleto, 005-explorador-hf, 006-gestor-descargas)
- **condicion-sin-test** — la comprobacion de generation tras downloadsList (store.svelte.ts:126) no la ejercita ningun test; el mock devuelve [] y no se puede observar la fusion (minor) (planes: 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas)
- **dato-inventado** — remove en Cancelled confía en estados Verified que cancel no reinicia y borra el modelo de una descarga posterior ya quitada de la lista (#87) (planes: 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas)
- **fixture-distinto-del-plan** — fetch_tamano_distinto no puede dar SizeMismatch con la descarga reanudable: un cuerpo corto 200 se reintenta y acaba en Network. (planes: 004-runtime-llamacpp, 005-explorador-hf, 006-gestor-descargas)
