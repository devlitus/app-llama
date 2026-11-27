---
schema: memoria-larga/1
tipo: errores
---

# Errores recurrentes

<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->

- **aserto-debil** — rejects.toBeDefined() pasaba con M1 (otro error) y sin mock; corregido (planes: 003-esqueleto, 004-runtime-llamacpp, 011-aserto-rechazo-api)
- **reserva-sin-test** — CA12 Cancelar y reseteo de installing/unlisten sin test de componente; se dio por difícil pero mockIPC(..., { shouldMockEvents: true }) de @tauri-apps/api 2.12.5 lo permite (planes: 002-falsos-positivos-hooks, 003-esqueleto, 004-runtime-llamacpp)
