---
schema: memoria-larga/1
tipo: errores
---

# Errores recurrentes

<!-- Generado por `node scripts/memory.mjs promote`. No editar a mano. -->

- **aserto-debil** — rejects.toBeDefined() pasaba con M1 (otro error) y sin mock; corregido (planes: 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf, 011-aserto-rechazo-api)
- **reserva-sin-test** — faltan quant_model_name_digits_not_matched (Qwen2.5-7B-Instruct.gguf) y quant_empty_path ("") de la tabla de T2 (planes: 002-falsos-positivos-hooks, 003-esqueleto, 004-runtime-llamacpp, 005-explorador-hf)
