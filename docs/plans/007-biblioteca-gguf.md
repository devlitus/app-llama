# 007 — Biblioteca local y lector de metadatos GGUF

- **Tipo:** feature
- **Estado:** borrador
- **Rama:** feat/007-biblioteca-gguf
- **Fase del roadmap:** 4. Biblioteca + GGUF ([docs/MVP.md](../MVP.md), funcionalidad C)

## Objetivo

Que el usuario vea en una pantalla Biblioteca todos sus modelos GGUF locales (descargados o importados) con su arquitectura, contexto de entrenamiento, nº de parámetros, cuantización y plantilla de chat, sin cargar el modelo. Desde ahí puede borrarlos, abrir su carpeta, elegir el directorio de modelos e importar un `.gguf` que ya tenga en disco.

## Contexto

### Punto de partida (supuesto)

Este plan da por hechos los planes 003 a 006, que se escriben en paralelo. En el momento de redactarlo no existe código en `crates/`, `src-tauri/` ni `ui/`, así que estas son las **suposiciones** que hace sobre ellos. Si alguna no se cumple, el developer lo comunica antes de empezar la tarea afectada (ver Preguntas abiertas):

- **003 (esqueleto):** workspace Cargo con el crate `core` en `crates/core` (sin Tauri), `src-tauri/` con Tauri 2 y `ui/` con Svelte 5 + TypeScript, Vitest, `@testing-library/svelte` y los scripts `lint`, `check` y `test`. Navegación con una entrada por pantalla (Explorar · Biblioteca · Ejecutar · Chat · Ajustes). `core` ya depende de `serde`, `thiserror` y `tempfile` (dev).
- **006 (descargas):** guarda los modelos en el **directorio de modelos** y mientras descarga escribe en un fichero temporal cuyo nombre **no** acaba en `.gguf` (p. ej. `*.gguf.part`). Si 006 ya define el ajuste `models_dir`, este plan lo reutiliza en lugar de crearlo (T6).
- **005 (explorador HF):** si ya existe un helper de formato de bytes en la UI (`ui/src/lib/format.ts`), se reutiliza.

### Módulos que se crean o tocan

| Módulo | Contenido |
|---|---|
| `crates/core/src/gguf/` | Lector binario, cabecera, metadatos, tensores, resumen del modelo y nombres de splits. Solo `std` + `thiserror` + `serde`. |
| `crates/core/src/config/` | Ajustes (`settings.toml`) e índice de modelos referenciados (`library.json`) en el directorio de datos de la app (crate `directories`). |
| `crates/core/src/library/` | Escaneo, borrado e importación. Se crea un módulo propio porque combina `gguf` y `config` y no encaja en ninguno de los dos. |
| `src-tauri/src/commands/library.rs` | Comandos Tauri finos, sin lógica. Plugins `tauri-plugin-dialog` y `tauri-plugin-opener`. |
| `ui/src/lib/library/` | Pantalla Biblioteca y sus componentes. |

### Formato GGUF (verificado el 2026-11-26)

Fuente: [ggml/docs/gguf.md](https://github.com/ggml-org/ggml/blob/master/docs/gguf.md), [llama.cpp/include/llama.h](https://github.com/ggml-org/llama.cpp/blob/master/include/llama.h) (`enum llama_ftype`) y [gguf-py/gguf/constants.py](https://github.com/ggml-org/llama.cpp/blob/master/gguf-py/gguf/constants.py).

- **Cabecera:** `magic` u32 = bytes `GGUF` (`0x46554747` leído en little-endian) · `version` u32 · `tensor_count` u64 · `metadata_kv_count` u64. v1 usaba u32 para los contadores: **no se soporta**. v2 pasó a u64. v3 añade big-endian, que el formato no señaliza. Se detecta por `version`: si leído en LE vale 2 o 3, el fichero es LE; si solo vale 2 o 3 leído en BE, es BE.
- **String:** `len` u64 + `len` bytes UTF-8, sin terminador nulo. Las claves son ASCII y miden 65 535 bytes como máximo.
- **Tipos de valor** (u32): 0 UINT8, 1 INT8, 2 UINT16, 3 INT16, 4 UINT32, 5 INT32, 6 FLOAT32, 7 BOOL (1 byte: 0 o 1, cualquier otro valor es inválido), 8 STRING, 9 ARRAY, 10 UINT64, 11 INT64, 12 FLOAT64.
- **Array:** tipo de elemento u32 + longitud u64 (en elementos) + elementos. Puede anidarse.
- **Info de tensor** (tras los KV): nombre (string) · `n_dims` u32 · `n_dims` × u64 · tipo u32 · offset u64. `GGML_MAX_DIMS` = 4. El nº de parámetros es la suma de los productos de las dimensiones: se calcula **sin leer los datos de los tensores**. La spec no define `general.parameter_count`; `general.size_label` (p. ej. `7B`) se muestra como apoyo si existe.
- **Claves que se usan:** `general.architecture` (string), `general.name` (string), `general.size_label` (string), `general.file_type` (u32), `{arch}.context_length` (entero; la spec dice u64 y llama.cpp escribe u32, así que se acepta cualquier entero no negativo), `tokenizer.chat_template` (string, Jinja), `split.no`, `split.count`, `split.tensors.count` (enteros; `gguf-split` los escribe como u16/u16/i32, así que se acepta cualquier entero no negativo).
- **`general.file_type` → etiqueta:** 0 F32, 1 F16, 2 Q4_0, 3 Q4_1, 7 Q8_0, 8 Q5_0, 9 Q5_1, 10 Q2_K, 11 Q3_K_S, 12 Q3_K_M, 13 Q3_K_L, 14 Q4_K_S, 15 Q4_K_M, 16 Q5_K_S, 17 Q5_K_M, 18 Q6_K, 19 IQ2_XXS, 20 IQ2_XS, 21 Q2_K_S, 22 IQ3_XS, 23 IQ3_XXS, 24 IQ1_S, 25 IQ4_NL, 26 IQ3_S, 27 IQ3_M, 28 IQ2_S, 29 IQ2_M, 30 IQ4_XS, 31 IQ1_M, 32 BF16, 36 TQ1_0, 37 TQ2_0, 38 MXFP4_MOE, 39 NVFP4, 40 Q1_0, 41 Q2_0, 42 F8_E4M3. 4, 5, 6, 33, 34 y 35 están retirados y 1024 es `GUESSED`: se tratan como desconocidos y se recurre al nombre del fichero.
- **Splits:** el nombre sigue el patrón `<base>-NNNNN-of-MMMMM.gguf` (5 dígitos, numeración desde `00001`). Los KV generales viajan en el shard 1; cada shard lleva sus propias infos de tensor.

### Decisiones de diseño

1. **Lectura acotada.** El lector trabaja sobre `Read + Seek`, conoce la longitud total del fichero y comprueba **antes** de cada lectura o salto que los bytes declarados caben en lo que queda. Así un fichero truncado da `Truncated` en lugar de un salto más allá del final o una reserva gigante. Las claves que no interesan (p. ej. `tokenizer.ggml.tokens`, con más de 100 000 strings) se **saltan** sin reservar memoria.
2. **Límites:** clave ≤ 65 535 bytes · string conservado ≤ 16 MiB · nombre de tensor ≤ 65 535 bytes · anidamiento de arrays ≤ 4 · `n_dims` entre 1 y 4 · `metadata_kv_count` ≤ bytes restantes / 13 y `tensor_count` ≤ bytes restantes / 28 (tamaños mínimos de un KV y de una info de tensor). Si se supera un límite, `Corrupt`. Las multiplicaciones y sumas de dimensiones usan `checked_*`: si se desbordan, `Corrupt`.
3. **Un fichero malo no rompe la biblioteca.** El escaneo nunca falla por un modelo: cada entrada lleva un `status` (`Ready`, `Incomplete { missing }`, `Invalid { reason }`, `Missing`).
4. **Identidad del modelo.** `id` = ruta absoluta del primer shard (o del único fichero), tal como la devuelve el escaneo. Las órdenes que reciben un `id` (borrar, abrir carpeta) **vuelven a escanear** y solo actúan si el `id` está en el resultado. Así la UI nunca puede pedir que se borre una ruta arbitraria.
5. **Origen.** `Local` (dentro del directorio de modelos) o `Referenced` (importado por referencia, guardado en `library.json`). Borrar un modelo `Referenced` solo lo quita del índice: **nunca** se borra un fichero del usuario fuera del directorio de modelos.
6. **Cambiar el directorio de modelos** solo actualiza el ajuste. No mueve los modelos existentes.
7. **Escaneo recursivo** con profundidad ≤ 4, sin seguir enlaces simbólicos. Solo se consideran ficheros cuya extensión es `.gguf` (sin distinguir mayúsculas). Los splits se agrupan por (directorio, base, total).
8. **Copia de importación** a `<models_dir>/<nombre>.gguf.importing` y `rename` al terminar. Si falla, se borra el temporal. Nunca se sobrescribe un fichero existente.

## Criterios de aceptación

- **CA1 — Metadatos v2 y v3.** Dado un GGUF sintético v2 o v3 (LE, y también v3 BE) con `general.architecture = "llama"`, `llama.context_length = 4096`, `general.file_type = 15`, `tokenizer.chat_template = "{{x}}"` y dos tensores de `[4096, 32000]` y `[4096]`, cuando se lee su resumen, entonces se obtienen la arquitectura `llama`, el contexto `4096`, `131 076 096` parámetros, la cuantización `Q4_K_M` y la plantilla `{{x}}`. Lo verifican los tests de T2–T4.
- **CA2 — Sin leer tensores.** Dado un GGUF sintético con 1 KiB de metadatos seguidos de 8 MiB de datos de tensor, cuando se lee su resumen a través de un lector que cuenta bytes, entonces se leen menos de `tamaño de cabecera + metadatos + infos de tensor + 64 KiB` bytes. Test `resumen_no_lee_datos_de_tensores` (T4).
- **CA3 — Ficheros corruptos o truncados.** Dado un GGUF válido truncado en **cada** offset entre 0 y su longitud − 1, cuando se lee, entonces siempre devuelve `Err` (`Truncated`, `NotGguf` o `Corrupt`) y nunca entra en pánico. Dado un directorio con un modelo válido, uno truncado y uno de bytes arbitrarios con extensión `.gguf`, cuando se escanea, entonces se obtienen 3 entradas: 1 `Ready` y 2 `Invalid` con motivo. Tests `truncar_en_cada_offset_devuelve_error` (T2) y `escanear_aisla_ficheros_invalidos` (T7).
- **CA4 — Splits agrupados.** Dados `m-00001-of-00003.gguf`, `m-00002-of-00003.gguf` y `m-00003-of-00003.gguf` válidos, cuando se escanea, entonces aparece **una** entrada con `shards = 3`, el tamaño igual a la suma de los tres y los parámetros sumados de los tres. Si falta el shard 2, la entrada es `Incomplete { missing: [2] }`. Tests de T5 y T7.
- **CA5 — Borrar.** Dado un modelo `Local` de 3 shards, cuando se borra por su `id`, entonces desaparecen los 3 ficheros y el resto del directorio queda intacto. Dado un modelo `Referenced`, cuando se borra, entonces sale de `library.json` y su fichero sigue en disco. Dado un `id` que no está en el escaneo, cuando se pide borrarlo, entonces se devuelve `UnknownModel` y no se toca ningún fichero. Tests de T8.
- **CA6 — Directorio de modelos.** Dado un directorio existente y escribible, cuando se elige como directorio de modelos, entonces se guarda en `settings.toml` y, al volver a cargar los ajustes, se lee el mismo valor. Dado un directorio inexistente o un fichero, cuando se elige, entonces se rechaza con un error y el ajuste no cambia. Sin ajuste guardado, el valor por defecto es `<data_dir>/models`. Tests de T6.
- **CA7 — Importar copiando.** Dado un `.gguf` válido fuera del directorio de modelos, cuando se importa en modo copia, entonces queda una copia byte a byte en el directorio de modelos, el original sigue intacto, el callback de progreso termina en `(total, total)` y el modelo aparece como `Local` en el siguiente escaneo. Si ya existe un fichero con ese nombre, se devuelve `AlreadyExists` y no se sobrescribe nada. Si la copia falla, no queda ningún `.importing`. Si es un shard, se importan todos los shards del grupo, o se devuelve `MissingShards` si falta alguno. Tests de T9.
- **CA8 — Importar por referencia.** Dado un `.gguf` válido fuera del directorio de modelos, cuando se importa por referencia, entonces se añade a `library.json` y aparece como `Referenced` sin copiar bytes. Importarlo dos veces no lo duplica. Si después se borra el fichero, el escaneo lo muestra como `Missing`. Un fichero inválido se rechaza con el error del parser. Tests de T9 y T7.
- **CA9 — Pantalla Biblioteca.** Dada una lista de 2 modelos (uno `Ready` y otro `Invalid`), cuando se renderiza la pantalla, entonces se ve una fila por modelo con nombre, arquitectura, parámetros (`7.2 B`), cuantización, contexto, tamaño y nº de shards, y la fila inválida muestra su motivo. Con la lista vacía se ve un estado vacío con el botón Importar. Borrar pide confirmación antes de invocar el comando. Tests de T11 y T12.
- **CA10 — Abrir carpeta y diálogos (comprobación manual en Windows y macOS).** Con la app en `npm run tauri dev`: "Abrir carpeta" abre el explorador del sistema con el fichero seleccionado; "Cambiar directorio" abre un selector de carpetas; "Importar" abre un selector filtrado por `.gguf`. El resultado se apunta en la memoria de la tarea T12.

## Tareas

### T1 — Lector binario acotado y errores GGUF
- **Ficheros/módulos:** `crates/core/src/gguf/mod.rs`, `crates/core/src/gguf/error.rs`, `crates/core/src/gguf/reader.rs`, `crates/core/src/gguf/test_support.rs` (`#[cfg(test)]`)
- **Depende de:** — (plan 003 hecho)
- **Cubre:** CA3 (base)
- **Subtareas:**
  - [ ] T1.1 `GgufError` con `thiserror`: `Io(std::io::Error)`, `NotGguf`, `UnsupportedVersion(u32)`, `Truncated { offset: u64, needed: u64 }`, `Corrupt(String)`. Mensajes de `Display` en español.
  - [ ] T1.2 `ByteReader<R: Read + Seek>` con `Endian { Little, Big }`, posición y longitud total: `read_u8/u16/u32/u64/i8/i16/i32/i64/f32/f64`, `read_bool` (solo 0 o 1), `read_string(max_len)`, `skip(n)` (con `seek_relative`) y `ensure_available(n)`.
  - [ ] T1.3 `GgufBuilder` de test: genera en memoria un GGUF v2/v3, LE o BE, con KV de cualquier tipo (también arrays anidados) e infos de tensor, y opcionalmente N bytes de datos de tensor. Los tests de T2–T9 lo reutilizan.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `lee_enteros_little_endian` | bytes `01 00 00 00` con `Endian::Little` | `read_u32` → `1` |
  | `lee_enteros_big_endian` | bytes `00 00 00 01` con `Endian::Big` | `read_u32` → `1` |
  | `lee_string_valido` | `05 00..00` + `hello` | `"hello"` |
  | `string_mas_largo_que_el_resto_es_truncated` | longitud 100 y 3 bytes detrás | `Err(Truncated { needed: 100, .. })` sin reservar 100 bytes |
  | `string_supera_maximo_es_corrupt` | longitud 20 con `max_len = 10` y 20 bytes detrás | `Err(Corrupt(_))` |
  | `string_utf8_invalido_es_corrupt` | longitud 2 + `FF FE` | `Err(Corrupt(_))` |
  | `bool_fuera_de_rango_es_corrupt` | byte `02` | `Err(Corrupt(_))` |
  | `bool_valido` | bytes `00`, `01` | `false`, `true` |
  | `skip_mas_alla_del_final_es_truncated` | 4 bytes, `skip(10)` | `Err(Truncated { .. })` y la posición no cambia |
  | `lectura_en_fichero_vacio_es_truncated` | 0 bytes, `read_u32` | `Err(Truncated { offset: 0, needed: 4 })` |
  | `builder_genera_cabecera_v3` | `GgufBuilder::v3()` sin KV | primeros 24 bytes: `GGUF`, `3u32`, `0u64`, `0u64` en LE |

### T2 — Cabecera y metadatos KV
- **Ficheros/módulos:** `crates/core/src/gguf/header.rs`, `crates/core/src/gguf/metadata.rs`
- **Depende de:** T1
- **Cubre:** CA1, CA3
- **Subtareas:**
  - [ ] T2.1 `read_header`: magic, detección de versión y endianness (v2/v3 LE, v3 BE), `tensor_count`, `metadata_kv_count` con la cota de bytes restantes.
  - [ ] T2.2 `read_metadata(reader, count, is_wanted: impl Fn(&str) -> bool)`: devuelve `BTreeMap<String, MetaValue>` solo con las claves pedidas. `MetaValue` = enteros, flotantes, `Bool` y `String`. Los arrays y las claves no pedidas se saltan validando su estructura (tipo de elemento, longitud y anidamiento ≤ 4). Las strings dentro de arrays se saltan leyendo solo su longitud.
  - [ ] T2.3 `MetaValue::as_u64()`: cualquier entero no negativo → `Some(u64)`; negativos, flotantes, bool y string → `None`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cabecera_v3_little_endian` | builder v3 LE, 2 tensores, 5 KV | `version 3`, `Endian::Little`, `tensor_count 2`, `kv_count 5` |
  | `cabecera_v2_little_endian` | builder v2 LE | `version 2` |
  | `cabecera_v3_big_endian` | builder v3 BE | `version 3`, `Endian::Big` |
  | `magic_incorrecto_es_not_gguf` | `GGML` + resto válido | `Err(NotGguf)` |
  | `version_1_no_soportada` | magic + `1u32` LE | `Err(UnsupportedVersion(1))` |
  | `version_futura_no_soportada` | magic + `4u32` LE | `Err(UnsupportedVersion(4))` |
  | `kv_count_imposible_es_corrupt` | v3 con `kv_count = u64::MAX` y 100 bytes detrás | `Err(Corrupt(_))` sin reservar memoria |
  | `lee_los_13_tipos_escalares` | tabla con un KV por tipo (0–8, 10–12) pedido | cada `MetaValue` con su valor exacto |
  | `salta_array_de_strings_no_pedido` | `tokenizer.ggml.tokens` con 1000 strings + `general.architecture` | mapa solo con `general.architecture` |
  | `salta_array_anidado` | array de arrays de u32 (profundidad 2) | se salta sin error |
  | `anidamiento_excesivo_es_corrupt` | arrays anidados con profundidad 5 | `Err(Corrupt(_))` |
  | `tipo_de_valor_desconocido_es_corrupt` | KV con tipo `13` | `Err(Corrupt(_))` |
  | `tipo_de_elemento_desconocido_es_corrupt` | array con tipo de elemento `99` | `Err(Corrupt(_))` |
  | `clave_demasiado_larga_es_corrupt` | clave de 65 536 bytes | `Err(Corrupt(_))` |
  | `as_u64_acepta_enteros_no_negativos` | `U16(4096)`, `I32(4096)`, `U64(8192)` | `Some(4096)`, `Some(4096)`, `Some(8192)` |
  | `as_u64_rechaza_negativos_y_no_enteros` | `I32(-1)`, `F32(1.0)`, `String("1")` | `None` en los tres |
  | `truncar_en_cada_offset_devuelve_error` | GGUF válido de ~600 bytes (KV de todos los tipos + 2 tensores) truncado en cada offset 0..len | todas las lecturas (cabecera + metadatos + tensores, vía `read_summary` cuando exista o `read_header`+`read_metadata` en T2) devuelven `Err` sin pánico |

### T3 — Infos de tensor y nº de parámetros
- **Ficheros/módulos:** `crates/core/src/gguf/tensors.rs`
- **Depende de:** T2
- **Cubre:** CA1, CA2, CA3
- **Subtareas:**
  - [ ] T3.1 `count_parameters(reader, tensor_count) -> Result<u64>`: lee nombre (que salta), `n_dims` (1–4), dimensiones, tipo y offset de cada tensor, sin guardar las infos. Producto y suma con `checked_*`.
  - [ ] T3.2 Ampliar `truncar_en_cada_offset_devuelve_error` para que cubra también la zona de infos de tensor.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `cuenta_parametros_de_dos_tensores` | `[4096, 32000]` y `[4096]` | `131_076_096` |
  | `sin_tensores_son_cero_parametros` | `tensor_count = 0` | `0` |
  | `n_dims_cero_es_corrupt` | tensor con `n_dims = 0` | `Err(Corrupt(_))` |
  | `n_dims_cinco_es_corrupt` | tensor con `n_dims = 5` | `Err(Corrupt(_))` |
  | `producto_desbordado_es_corrupt` | dims `[u64::MAX, 2]` | `Err(Corrupt(_))` |
  | `suma_desbordada_es_corrupt` | dos tensores de `[u64::MAX / 2 + 1]` | `Err(Corrupt(_))` |
  | `tensor_count_imposible_es_corrupt` | `tensor_count = 1_000_000` con 64 bytes restantes | `Err(Corrupt(_))` |
  | `info_de_tensor_truncada_es_truncated` | fichero cortado a mitad de las dimensiones del 2.º tensor | `Err(Truncated { .. })` |

### T4 — Resumen del modelo
- **Ficheros/módulos:** `crates/core/src/gguf/summary.rs`, `crates/core/src/gguf/file_type.rs`
- **Depende de:** T3
- **Cubre:** CA1, CA2
- **Subtareas:**
  - [ ] T4.1 `GgufSummary` (`Serialize`): `version`, `architecture: Option<String>`, `name: Option<String>`, `context_length: Option<u64>`, `parameter_count: u64`, `size_label: Option<String>`, `quantization: Option<String>`, `chat_template: Option<String>`, `split: Option<SplitMeta { no, count, tensors_count }>`, `tensor_count`.
  - [ ] T4.2 `read_summary(path)` y `read_summary_from(reader, file_name)`: piden solo las claves del contexto y cualquier clave que acabe en `.context_length`. Esa clave se resuelve **después** de leer `general.architecture`, porque el orden de los KV no está garantizado.
  - [ ] T4.3 `file_type_label(u32) -> Option<&'static str>` con la tabla del Contexto. `quantization_from_file_name(&str)`: busca, sin distinguir mayúsculas, un segmento separado por `-`, `.` o `_` del nombre que coincida con una etiqueta de la tabla (gana la más larga: `Q4_K_M` antes que `Q4_K`). Se usa si `file_type` falta, es desconocido o vale 1024.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `resumen_completo_v3` | KV del CA1 + 2 tensores | todos los campos de CA1 |
  | `resumen_completo_v2` | lo mismo en v2 | lo mismo con `version 2` |
  | `resumen_big_endian` | lo mismo en v3 BE | lo mismo |
  | `contexto_antes_que_arquitectura` | `qwen2.context_length` escrito antes de `general.architecture = "qwen2"` | `context_length = Some(..)` |
  | `contexto_de_otra_arquitectura_se_ignora` | arch `llama` con solo `qwen2.context_length` | `context_length = None` |
  | `contexto_u32_y_u64` | `llama.context_length` como `U32(8192)` y como `U64(8192)` | `Some(8192)` en ambos |
  | `sin_arquitectura_no_falla` | sin `general.architecture` | `architecture = None`, `context_length = None`, `Ok` |
  | `file_type_conocidos` | tabla: 0, 1, 7, 15, 18, 19, 30, 32, 38, 42 | `F32`, `F16`, `Q8_0`, `Q4_K_M`, `Q6_K`, `IQ2_XXS`, `IQ4_XS`, `BF16`, `MXFP4_MOE`, `F8_E4M3` |
  | `file_type_retirados_o_desconocidos` | 4, 5, 33, 43, 1024 | `None` |
  | `cuantizacion_desde_nombre` | tabla: `llama-3-8b.Q4_K_M.gguf`, `qwen2.5-7b-instruct-q5_k_s.gguf`, `phi-IQ4_XS-00001-of-00002.gguf`, `model-f16.gguf`, `modelo.gguf` | `Q4_K_M`, `Q5_K_S`, `IQ4_XS`, `F16`, `None` |
  | `file_type_desconocido_recurre_al_nombre` | `file_type = 1024`, nombre `x-Q8_0.gguf` | `quantization = Some("Q8_0")` |
  | `plantilla_de_chat_grande` | `tokenizer.chat_template` de 200 KiB | se conserva entera |
  | `plantilla_de_chat_excesiva_es_corrupt` | `tokenizer.chat_template` declarada de 17 MiB (con los bytes presentes) | `Err(Corrupt(_))` |
  | `split_meta_con_tipos_de_gguf_split` | `split.no = U16(0)`, `split.count = U16(3)`, `split.tensors.count = I32(10)` | `SplitMeta { no: 0, count: 3, tensors_count: 10 }` |
  | `resumen_no_lee_datos_de_tensores` | 1 KiB de metadatos + 8 MiB de datos de tensor, lector que cuenta bytes | bytes leídos < offset de inicio de datos + 64 KiB |

### T5 — Nombres de splits y agrupación
- **Ficheros/módulos:** `crates/core/src/gguf/split_name.rs`
- **Depende de:** T1 (solo el módulo)
- **Cubre:** CA4
- **Subtareas:**
  - [ ] T5.1 `parse_shard_name(file_name) -> Option<ShardName { base, index, total }>`: patrón `<base>-NNNNN-of-MMMMM.gguf` (exactamente 5 dígitos, extensión sin distinguir mayúsculas, `1 ≤ index ≤ total`), sin crate `regex`.
  - [ ] T5.2 `group_files(paths: &[PathBuf]) -> Vec<FileGroup>`: agrupa por (directorio padre, base, total). Un fichero que no es shard forma su propio grupo. Cada grupo lleva los shards ordenados por índice y los índices que faltan. Salida ordenada por ruta del primer fichero.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `nombre_de_shard_valido` | `Qwen-7B-Q4_K_M-00002-of-00003.gguf` | `base "Qwen-7B-Q4_K_M"`, `index 2`, `total 3` |
  | `extension_en_mayusculas` | `m-00001-of-00002.GGUF` | `Some(index 1, total 2)` |
  | `no_shards` | tabla: `m.gguf`, `m-0001-of-0002.gguf` (4 dígitos), `m-00000-of-00002.gguf`, `m-00003-of-00002.gguf`, `m-00001-of-00002.bin`, `-00001-of-00002.gguf` (base vacía) | `None` en todos |
  | `agrupa_tres_shards_desordenados` | shards 3, 1, 2 del mismo modelo | 1 grupo con 3 rutas en orden 1, 2, 3 y `missing = []` |
  | `detecta_shard_que_falta` | shards 1 y 3 de 3 | `missing = [2]` |
  | `misma_base_en_distinto_directorio` | `a/m-00001-of-00001.gguf` y `b/m-00001-of-00001.gguf` | 2 grupos |
  | `misma_base_distinto_total` | `m-00001-of-00002.gguf` y `m-00001-of-00003.gguf` | 2 grupos |
  | `fichero_simple_es_su_propio_grupo` | `m.gguf` y `n.gguf` | 2 grupos de 1 fichero |
  | `rutas_con_path_no_strings` | rutas creadas con `Path::join` | mismo resultado en Windows y macOS |

### T6 — Ajustes e índice de referencias
- **Ficheros/módulos:** `crates/core/src/config/mod.rs`, `crates/core/src/config/settings.rs`, `crates/core/src/config/library_index.rs`, `crates/core/Cargo.toml` (`directories`, `toml`, `serde_json`)
- **Depende de:** — (plan 003 hecho; si 006 ya define los ajustes, se amplían en lugar de crearlos)
- **Cubre:** CA6, CA8 (persistencia)
- **Subtareas:**
  - [ ] T6.1 `AppPaths { config_file, library_index, default_models_dir }`, construido desde una ruta base. `AppPaths::from_system()` usa `directories::ProjectDirs`; los tests usan `AppPaths::under(tempdir)`.
  - [ ] T6.2 `Settings { models_dir: Option<PathBuf> }` (TOML) con `load`, `save` (escritura a temporal + `rename`) y `models_dir_or_default`. Un fichero inexistente da los valores por defecto. Un TOML inválido da `ConfigError::Parse`, sin sobrescribir el fichero.
  - [ ] T6.3 `set_models_dir(path)`: comprueba que existe, que es directorio y que es escribible (crea y borra un fichero de prueba). Si no, `ConfigError::InvalidModelsDir` y no guarda.
  - [ ] T6.4 `LibraryIndex { references: Vec<PathBuf> }` (JSON) con `load`, `save`, `add` (idempotente) y `remove`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `ajustes_por_defecto_sin_fichero` | tempdir vacío | `models_dir_or_default() == <base>/models` |
  | `guardar_y_cargar_ajustes` | `set_models_dir(tmp/x)` y `load` | `models_dir == Some(tmp/x)` |
  | `toml_invalido_es_error_y_no_se_sobrescribe` | `settings.toml` con `models_dir = [` | `Err(Parse)` y el contenido sigue igual |
  | `directorio_inexistente_se_rechaza` | `tmp/no-existe` | `Err(InvalidModelsDir)`, el ajuste no cambia |
  | `fichero_como_directorio_se_rechaza` | ruta a un fichero | `Err(InvalidModelsDir)` |
  | `indice_vacio_sin_fichero` | tempdir vacío | `references == []` |
  | `anadir_referencia_es_idempotente` | `add(p)` dos veces | `references == [p]` |
  | `quitar_referencia` | `[p, q]`, `remove(p)` | `[q]` |
  | `json_invalido_es_error` | `library.json` con `{` | `Err(Parse)` |
  | `rutas_con_espacios_y_no_ascii` | `tmp/"Modelos ñ"` | se guarda y se carga igual |

### T7 — Escaneo de la biblioteca
- **Ficheros/módulos:** `crates/core/src/library/mod.rs`, `crates/core/src/library/entry.rs`, `crates/core/src/library/scan.rs`
- **Depende de:** T4, T5, T6
- **Cubre:** CA3, CA4, CA8
- **Subtareas:**
  - [ ] T7.1 `LibraryEntry` (`Serialize`): `id`, `display_name` (`general.name` o, si no hay, la base del fichero), `origin: Local | Referenced`, `files: Vec<PathBuf>`, `total_bytes`, `shards`, `summary: Option<GgufSummary>` (los parámetros sumados en todos los shards) y `status: Ready | Incomplete { missing } | Invalid { reason } | Missing`.
  - [ ] T7.2 `list_gguf_files(models_dir)`: recursivo con profundidad ≤ 4, sin seguir enlaces simbólicos, solo `.gguf`. Si `models_dir` no existe, la lista es vacía (no es error).
  - [ ] T7.3 `scan(models_dir, index) -> Vec<LibraryEntry>`: agrupa, lee el resumen del shard 1 y los parámetros de cada shard, y añade las referencias del índice (también agrupadas, con `Missing` si falta el fichero). El error de un modelo solo afecta a su entrada. Orden: `display_name` sin distinguir mayúsculas y después `id`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `directorio_inexistente_da_lista_vacia` | ruta que no existe | `Ok(vec![])` |
  | `ignora_no_gguf_y_temporales` | `a.gguf`, `b.gguf.part`, `c.txt`, `d.gguf.importing` | 1 entrada (`a`) |
  | `escanea_subdirectorios_hasta_profundidad_4` | modelos a profundidad 1, 4 y 5 | entradas de 1 y 4; la de 5 no |
  | `escanear_aisla_ficheros_invalidos` | válido + truncado + 64 bytes arbitrarios con semilla fija, los tres `.gguf` | 3 entradas: 1 `Ready`, 2 `Invalid` con `reason` no vacío |
  | `split_completo_es_una_entrada` | 3 shards válidos (con 2, 3 y 1 tensores) | 1 entrada, `shards 3`, `total_bytes` = suma, `parameter_count` = suma |
  | `split_incompleto` | shards 1 y 3 de 3 | `Incomplete { missing: [2] }` |
  | `shard_intermedio_corrupto_invalida_el_grupo` | shard 2 truncado | `Invalid { .. }` con el nombre del shard en `reason` |
  | `nombre_visible_sin_general_name` | `llama-7b.Q4_K_M.gguf` sin `general.name` | `display_name == "llama-7b.Q4_K_M"` |
  | `referencia_existente` | índice con un `.gguf` válido fuera de `models_dir` | entrada `Referenced`, `Ready` |
  | `referencia_desaparecida` | índice con una ruta que no existe | entrada `Referenced`, `Missing` |
  | `orden_estable` | `b`, `A`, `c` | `A`, `b`, `c` |

### T8 — Borrar un modelo
- **Ficheros/módulos:** `crates/core/src/library/delete.rs`, `crates/core/src/library/error.rs`
- **Depende de:** T7
- **Cubre:** CA5
- **Subtareas:**
  - [ ] T8.1 `LibraryError` (`thiserror`): `UnknownModel(String)`, `OutsideModelsDir(PathBuf)`, `PartialDelete { remaining: Vec<PathBuf> }`, `AlreadyExists(PathBuf)`, `MissingShards(Vec<u32>)`, `InvalidGguf(GgufError)`, `Config(ConfigError)`, `Io(std::io::Error)`.
  - [ ] T8.2 `delete_model(id, models_dir, index_path)`: escanea y busca el `id`. Si es `Local`, comprueba que cada fichero canonizado empieza por `models_dir` canonizado y borra todos los shards; si alguno falla, devuelve `PartialDelete` con los que quedan. Después borra los directorios que se hayan quedado vacíos entre el fichero y `models_dir` (este nunca). Si es `Referenced`, solo lo quita del índice.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `borra_todos_los_shards` | modelo de 3 shards + otro modelo | quedan solo los ficheros del otro modelo |
  | `borra_modelo_de_un_fichero` | `m.gguf` | el fichero ya no existe |
  | `id_desconocido_no_toca_nada` | `id` = ruta de un fichero fuera de la biblioteca | `Err(UnknownModel)`, el fichero sigue |
  | `referencia_solo_sale_del_indice` | modelo `Referenced` | `library.json` sin la ruta y el fichero sigue en disco |
  | `borra_directorio_vacio_resultante` | `models/org/repo/m.gguf` único | no existen ni `models/org/repo` ni `models/org`; `models` sí |
  | `no_borra_directorio_con_otros_ficheros` | `models/org/m.gguf` + `models/org/readme.txt` | `models/org` sigue |
  | `modelo_invalido_tambien_se_puede_borrar` | `.gguf` truncado (entrada `Invalid`) | se borra |
  | `split_incompleto_se_borra_entero` | shards 1 y 3 de 3 | se borran los dos |

### T9 — Importar un `.gguf` existente
- **Ficheros/módulos:** `crates/core/src/library/import.rs`
- **Depende de:** T7, T8 (errores)
- **Cubre:** CA7, CA8
- **Subtareas:**
  - [ ] T9.1 `resolve_import_set(path)`: valida que es `.gguf`, lee su resumen (si falla, `InvalidGguf`) y, si es un shard, localiza todos los del grupo en su directorio (si falta alguno, `MissingShards`).
  - [ ] T9.2 `import_copy(path, models_dir, on_progress: impl FnMut(u64, u64))`: comprueba antes que no exista ningún destino (`AlreadyExists`). Copia por bloques de 1 MiB a `<nombre>.importing`, notifica `(copiados, total)` acumulado sobre todos los shards y hace `rename`. Si hay un error, borra los temporales y los ficheros ya renombrados de esta importación. Si el origen ya está dentro de `models_dir`, devuelve `AlreadyExists`.
  - [ ] T9.3 `import_reference(path, models_dir, index_path)`: añade el primer shard al índice (idempotente). Si está dentro de `models_dir`, no añade nada (ya aparece como `Local`) y devuelve `Ok`.
  - [ ] T9.4 Inyectar la escritura en un trait `CopySink` (o pasar un `Write` fábrica) para poder simular un fallo a mitad de copia en los tests sin depender del disco.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `copia_byte_a_byte` | `.gguf` válido de 3 MiB fuera de `models_dir` | destino idéntico (comparación de bytes) y original intacto |
  | `progreso_monotono_y_final` | lo mismo | llamadas crecientes y la última `(3 MiB, 3 MiB)` |
  | `no_sobrescribe_existente` | destino con el mismo nombre y otro contenido | `Err(AlreadyExists)` y el destino sin cambios |
  | `fallo_a_mitad_no_deja_temporales` | `CopySink` que falla tras 1 MiB | `Err(Io)` y ningún `.importing` ni destino |
  | `importa_todos_los_shards` | elegir el shard 2 de 3 | se copian los 3 |
  | `shards_incompletos_se_rechazan` | shards 1 y 2 de 3 | `Err(MissingShards([3]))` y nada copiado |
  | `fichero_invalido_se_rechaza` | `.gguf` con magic `XXXX` | `Err(InvalidGguf(NotGguf))` |
  | `extension_no_gguf_se_rechaza` | `modelo.bin` válido | `Err(InvalidGguf(_))` o error de extensión, sin copiar |
  | `referencia_se_anade_al_indice` | `.gguf` válido externo | índice con la ruta y escaneo con 1 `Referenced` |
  | `referencia_duplicada_no_se_repite` | importar dos veces | índice con 1 ruta |
  | `referencia_dentro_de_models_dir_no_se_anade` | fichero ya en `models_dir` | `Ok` e índice vacío |

### T10 — Comandos Tauri de la biblioteca
- **Ficheros/módulos:** `src-tauri/src/commands/library.rs`, `src-tauri/src/lib.rs` (registro), `src-tauri/Cargo.toml` (`tauri-plugin-dialog`, `tauri-plugin-opener`), `src-tauri/capabilities/default.json`, `ui/package.json` (`@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-opener`)
- **Depende de:** T6, T7, T8, T9
- **Cubre:** CA5–CA8, CA10
- **Subtareas:**
  - [ ] T10.1 Comandos `library_list`, `library_delete(id)`, `library_import(path, mode: "copy" | "reference")`, `library_reveal(id)`, `settings_get_models_dir` y `settings_set_models_dir(path)`. Cada uno solo traduce argumentos y errores (`String` en español) y llama a `core`. Las operaciones de disco van en `tauri::async_runtime::spawn_blocking`.
  - [ ] T10.2 `library_import` en modo copia emite el evento `library://import-progress` con `{ copied, total }`, como mucho una vez cada 1 MiB.
  - [ ] T10.3 `library_reveal(id)` resuelve el `id` con un escaneo y llama a `reveal_item_in_dir` de `tauri-plugin-opener` (Rust). La UI nunca pasa rutas a abrir.
  - [ ] T10.4 Capabilities: `dialog:allow-open`, `dialog:allow-message`, `opener:allow-reveal-item-in-dir`. **No** se concede `opener:allow-open-path`.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `modo_importacion_desde_texto` | `"copy"`, `"reference"`, `"move"` | `Copy`, `Reference`, `Err` con mensaje en español |
  | `error_de_biblioteca_a_mensaje` | `LibraryError::AlreadyExists(p)` | mensaje que contiene el nombre del fichero |
  | `progreso_se_limita_por_mib` | callback con 0, 512 KiB, 1 MiB, 1.5 MiB, 3 MiB (total 3 MiB) | se emiten 0, 1 MiB y 3 MiB (siempre el final) |

  (La lógica testeable de los comandos vive en funciones puras del mismo módulo; la integración con la ventana se verifica en CA10.)

### T11 — Pantalla Biblioteca (lista y detalle)
- **Ficheros/módulos:** `ui/src/lib/library/LibraryView.svelte`, `ui/src/lib/library/ModelRow.svelte`, `ui/src/lib/library/ModelDetail.svelte`, `ui/src/lib/library/api.ts`, `ui/src/lib/library/types.ts`, `ui/src/lib/format.ts` (reutilizar o crear), entrada de navegación de 003
- **Depende de:** T10
- **Cubre:** CA9
- **Subtareas:**
  - [ ] T11.1 Tipos TS espejo de `LibraryEntry` y `api.ts` con `invoke` tipado.
  - [ ] T11.2 `formatParams(n)` y `formatBytes(n)` (reutilizar el de 005 si existe).
  - [ ] T11.3 Tabla con nombre, arquitectura, parámetros (o `size_label` si `parameter_count` es 0), cuantización, contexto, tamaño, shards y estado. Detalle al seleccionar, con la plantilla de chat en un bloque monoespaciado plegable. Estados vacío, cargando y error.
- **Tests unitarios** (Vitest, `mockIPC` de `@tauri-apps/api/mocks`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `format_params_unidades` | tabla: `0`, `999`, `494_032_768`, `7_241_732_096`, `70_553_706_496` | `—`, `999`, `494 M`, `7.2 B`, `70.6 B` |
  | `format_bytes_unidades` | tabla: `0`, `1023`, `1_048_576`, `4_368_439_584` | `0 B`, `1023 B`, `1.0 MiB`, `4.07 GiB` |
  | `muestra_una_fila_por_modelo` | `library_list` → 2 entradas | 2 filas con nombre, `llama`, `7.2 B`, `Q4_K_M`, `4096` |
  | `fila_invalida_muestra_motivo` | entrada `Invalid { reason: "fichero truncado" }` | la fila contiene `fichero truncado` |
  | `fila_incompleta_muestra_shards_que_faltan` | `Incomplete { missing: [2] }` | texto con `falta el shard 2` |
  | `estado_vacio_con_boton_importar` | `library_list` → `[]` | mensaje de biblioteca vacía y botón `Importar` |
  | `error_al_listar_se_muestra` | `library_list` rechaza con `"sin permiso"` | aviso con `sin permiso` |
  | `detalle_muestra_plantilla_de_chat` | clic en una fila con `chat_template` | bloque con la plantilla |

### T12 — Acciones de la Biblioteca en la UI
- **Ficheros/módulos:** `ui/src/lib/library/LibraryActions.svelte`, `ui/src/lib/library/ImportDialog.svelte`, `ui/src/lib/library/LibraryView.svelte`
- **Depende de:** T11
- **Cubre:** CA9, CA10
- **Subtareas:**
  - [ ] T12.1 Borrar: confirmación (`ask` de `plugin-dialog`) con el nombre, el nº de ficheros y el tamaño; para un `Referenced`, el texto aclara que el fichero no se borra. Después, `library_delete` y recarga.
  - [ ] T12.2 Abrir carpeta → `library_reveal(id)`.
  - [ ] T12.3 Directorio de modelos: muestra el actual y el botón "Cambiar" (`open({ directory: true })`) → `settings_set_models_dir` → recarga.
  - [ ] T12.4 Importar: `open({ filters: [{ name: "GGUF", extensions: ["gguf"] }] })` → elegir "Copiar a la biblioteca" o "Usar en su ubicación" → `library_import`, con barra de progreso escuchando `library://import-progress`.
  - [ ] T12.5 Comprobación manual de CA10 en Windows (y en macOS si hay máquina disponible), apuntada en la memoria de la tarea.
- **Tests unitarios** (Vitest, con `mockIPC` y mocks de los plugins):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `borrar_pide_confirmacion` | clic en Borrar y `ask` → `false` | no se invoca `library_delete` |
  | `borrar_confirmado_invoca_y_recarga` | `ask` → `true` | `library_delete` con el `id` y después `library_list` |
  | `confirmacion_de_referencia_avisa_que_no_borra_fichero` | entrada `Referenced` | el texto de `ask` contiene `no se borrará el fichero` |
  | `abrir_carpeta_pasa_id` | clic en Abrir carpeta | `library_reveal` con el `id`, nunca una ruta suelta |
  | `cambiar_directorio_cancelado_no_hace_nada` | `open` → `null` | no se invoca `settings_set_models_dir` |
  | `cambiar_directorio_invalido_muestra_error` | `settings_set_models_dir` rechaza | aviso con el mensaje y el directorio mostrado no cambia |
  | `importar_por_referencia` | archivo elegido y modo "Usar en su ubicación" | `library_import` con `mode: "reference"` |
  | `importar_copiando_muestra_progreso` | evento `{ copied: 50, total: 100 }` | barra al 50 % |
  | `importar_error_ya_existe` | `library_import` rechaza con `ya existe` | aviso con el mensaje y la lista sin cambios |

## Fuera de alcance

- Mover los modelos existentes al cambiar el directorio de modelos.
- Borrar del disco un fichero importado por referencia.
- Caché de metadatos entre escaneos (se relee la cabecera en cada escaneo) y vigilancia del directorio (`notify`).
- Inferir la cuantización a partir de los tipos de los tensores (se usan `general.file_type` y el nombre del fichero).
- GGUF v1 y ficheros `mmproj` (multimodal: fuera del MVP).
- Comprobar el espacio libre antes de importar copiando (si 006 ya expone esa utilidad, se puede añadir en otro plan).
- Bloquear el borrado de un modelo que está usando `llama-server` (plan de ejecución, fase 5).
- Editar metadatos o convertir/recuantizar modelos.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| Un fichero malicioso declara longitudes enormes y provoca una reserva de memoria o un bucle largo | Comprobación de bytes restantes antes de cada lectura, límites del Contexto, cotas de `kv_count`/`tensor_count` y test de truncado en cada offset |
| Saltar vocabularios de 150 000+ strings es lento con un `seek` por string | `BufReader` + `seek_relative` (no vacía el buffer); si una medición manual supera 1 s por modelo, se apunta en la memoria y se abre un plan de caché |
| Borrar fuera del directorio de modelos por un `id` manipulado o por enlaces simbólicos | Solo se aceptan `id` del escaneo, se comprueba la contención con rutas canonizadas y no se siguen enlaces simbólicos |
| En Windows no se puede borrar un modelo abierto por `llama-server` | `PartialDelete` con los ficheros que quedan y mensaje claro en la UI |
| 006 escribe descargas en curso con extensión `.gguf` y salen como `Invalid` | Suposición explícita en el Contexto; si no se cumple, se ignoran sus temporales por nombre en T7 |
| 003–006 definen la estructura de `core` o los ajustes de otra forma | El developer lo comprueba al empezar T6/T10 y se adapta a lo que exista sin duplicar (ver Preguntas abiertas) |
| Una etiqueta nueva de `llama_ftype` no está en la tabla | Se muestra la del nombre del fichero o "desconocida"; la tabla se amplía en un plan aparte |
| La copia de un modelo de 50 GB tarda minutos | Progreso por evento y operación en `spawn_blocking`; cancelar la importación queda fuera de alcance |

## Preguntas abiertas

1. **Estructura de `core`:** ¿`gguf`, `config` y `library` son módulos de un único crate `crates/core` (`crates/core/src/gguf/`), como supone este plan, o crates separados (`crates/core/gguf`)? Depende de cómo quede 003.
2. **Módulo `library`:** este plan crea `crates/core/src/library/` para escaneo, borrado e importación, además de `gguf` y `config`. ¿Se acepta, o hay que repartir esa lógica entre `gguf` y `config`?
3. **Ajuste `models_dir`:** si 006 ya lo define (con otro nombre o fichero), ¿se reutiliza tal cual? Este plan asume que sí y que T6 solo añade lo que falte.
4. **Temporales de 006:** ¿qué extensión usan las descargas en curso? El plan supone que no acaba en `.gguf`.
5. **Destino de la copia al importar:** ¿la raíz de `models_dir` (supuesto actual) o una subcarpeta (`imported/`) para no mezclarlos con las descargas de 006?
6. **Unidades de tamaño:** ¿binarias (`GiB`, supuesto actual) o decimales (`GB`, como Hugging Face)? Debe coincidir con lo que muestre el explorador de 005.
