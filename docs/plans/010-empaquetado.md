# 010 — Empaquetado: instaladores, iconos, release y firma preparada

- **Tipo:** chore
- **Estado:** borrador
- **Rama:** chore/010-empaquetado
- **Fase del roadmap:** 7. Empaquetado ([docs/MVP.md](../MVP.md))

## Objetivo

Que el usuario pueda descargar de una release de GitHub un instalador `.msi` o `.exe` (NSIS) para Windows y un `.dmg` universal para macOS, con icono y metadatos propios, y que sepa instalarlo aunque no esté firmado. Publicar una versión se reduce a empujar un tag `vX.Y.Z`: el resultado es un borrador de release con los instaladores y sus SHA256. La firma de macOS (con notarización) y la de Windows se activan solo con añadir secretos al repositorio.

## Contexto

### Punto de partida (se suponen hechos los planes 003–009)

- El plan 003 creó el proyecto Tauri 2 (`src-tauri/`, `ui/` con Svelte 5 + TS) y una CI de build para Windows y macOS. Este plan **no cambia** `ci.yml` ni el build de 003. Añade un workflow aparte, `release.yml`.
- `.github/workflows/ci.yml` usa `actions/checkout@v4` y `actions/setup-node@v4` (Node 24). `release.yml` usa las mismas versiones.
- El quality gate (`scripts/quality-gate.mjs`) ejecuta `node --test "scripts/**/*.test.mjs"`. La lógica testeable de este plan va en `scripts/lib/*.mjs`, con sus tests al lado, igual que `push-guard.mjs`.
- El escáner de secretos del gate (`scripts/lib/secrets.mjs`) recorre `scripts/`. **Los fixtures de los tests usan valores falsos y cortos** (`"cert-falso"`, `"clave-falsa"`), nunca cadenas que parezcan base64 o tokens reales.
- Linux y el auto-update quedan fuera del MVP (MVP §6).

### Documentación de Tauri 2 verificada (2026-11-26)

- Firma de macOS: <https://v2.tauri.app/distribute/sign/macos/>
  - Variables de entorno: `APPLE_CERTIFICATE` (`.p12` en base64), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`. Para notarizar: `APPLE_ID`, `APPLE_PASSWORD` (contraseña de aplicación) y `APPLE_TEAM_ID`, o bien `APPLE_API_ISSUER`, `APPLE_API_KEY` y `APPLE_API_KEY_PATH`.
  - `bundle > macOS > signingIdentity` admite la pseudoidentidad `"-"` (firma ad hoc). Apple Silicon exige firma para apps descargadas de Internet. La firma ad hoc **no** evita que Gatekeeper pida autorizar la app en Privacidad y seguridad.
  - Solo un certificado *Developer ID Application* permite notarizar. Una cuenta gratuita no puede.
- Firma de Windows: <https://v2.tauri.app/distribute/sign/windows/>
  - Claves `bundle > windows > certificateThumbprint`, `digestAlgorithm` (`sha256`) y `timestampUrl`, o un `signCommand` propio.
  - En CI, el `.pfx` en base64 va en `WINDOWS_CERTIFICATE` y su contraseña en `WINDOWS_CERTIFICATE_PASSWORD`. Se importa con `Import-PfxCertificate`.
  - Desde 2024, un certificado EV ya no da reputación inmediata en SmartScreen: un build firmado y nuevo puede seguir mostrando el aviso.
- Instalador de Windows: <https://v2.tauri.app/distribute/windows-installer/>
  - El `.msi` (WiX) **solo se construye en Windows**.
  - NSIS instala por defecto para el usuario actual (`installMode: currentUser`), sin permisos de administrador.
  - `webviewInstallMode` vale por defecto `downloadBootstrapper`, el instalador más pequeño.
- Esquema de configuración: <https://v2.tauri.app/reference/config/>
  - `identifier` es obligatorio. Solo admite `A-Z a-z 0-9 - .`, y `tauri build` rechaza `com.tauri.dev`.
  - Si se omite `version`, se usa la de `Cargo.toml`.
  - `bundle.active` vale `false` por defecto.
  - `bundle.targets` admite `msi`, `nsis`, `app` y `dmg`.
  - `bundle.macOS.hardenedRuntime` vale `true` por defecto. `bundle.macOS.minimumSystemVersion` vale `"10.13"` por defecto.
  - `WixConfig.version` sigue el formato `major.minor.patch.build`, con major y minor ≤ 255 y patch y build ≤ 65535. Si no se indica, se deriva de la versión de la app.
  - `wix.upgradeCode` debe ser único entre aplicaciones.
- DMG: <https://v2.tauri.app/distribute/dmg/>. En CI no se aplican las posiciones de los iconos (tauri#1731), así que se deja el diseño por defecto.
- Iconos: <https://v2.tauri.app/develop/icons/>. `tauri icon [fuente]` parte de un PNG o SVG cuadrado con transparencia y genera `icon.icns`, `icon.ico` y varios `*.png` en `src-tauri/icons`.
- Pipeline de GitHub: <https://v2.tauri.app/distribute/pipelines/github/>
  - Acción `tauri-apps/tauri-action@v1`, con `permissions: contents: write`.
  - Entradas: `releaseId`, `tagName`, `releaseDraft`, `projectPath`, `tauriScript`, `args` y `uploadUpdaterJson`. Salidas: `artifactPaths` y `appVersion` (<https://github.com/tauri-apps/tauri-action>).
  - El ejemplo oficial construye macOS con dos entradas de matriz, `aarch64` y `x86_64`.

### Decisiones de diseño

1. **macOS: un único `.dmg` universal** (`--target universal-apple-darwin`) en lugar de dos `.dmg` (arm64 y x64). Motivos:
   - El usuario no tiene que saber qué procesador tiene su Mac.
   - Un build x64 abierto en Apple Silicon corre bajo Rosetta. Ahí, la detección de hardware de la Fase 1 vería una CPU x86_64 sin Metal y descargaría la release equivocada de llama.cpp. El universal siempre corre nativo.
   - Se firma y notariza **un** artefacto por release, no dos.
   - Coste: el binario de la app ocupa unas dos veces más (de unos 10 MB a unos 20 MB) y el job de macOS compila dos veces. Los modelos ocupan GB, así que el tamaño no importa.
   - Los targets `aarch64-apple-darwin` y `x86_64-apple-darwin` se añaden con `rustup target add` **solo en el workflow**, no en `rust-toolchain.toml`, para no obligar a cada desarrollador de Windows a descargarlos.
2. **Windows: `.msi` y NSIS (`-setup.exe`)**. NSIS es el recomendado para usuarios sin permisos de administrador (`currentUser`). El `.msi` sirve para despliegues gestionados. `webviewInstallMode` se queda en `downloadBootstrapper`, porque la app necesita Internet de todos modos (HF, llama.cpp).
3. **Versión: una sola fuente, `src-tauri/Cargo.toml`.**
   - `tauri.conf.json` **no** define `version`, así que Tauri la toma de Cargo.
   - El tag debe ser exactamente `v<versión>`.
   - Para que el `.msi` se pueda construir, el MVP solo admite `X.Y.Z` numérico (X, Y ≤ 255; Z ≤ 65535), sin prerelease.
4. **Configuración por plataforma** con los ficheros que Tauri fusiona automáticamente: `tauri.windows.conf.json` y `tauri.macos.conf.json` (JSON Merge Patch sobre `tauri.conf.json`).
5. **Firma opcional y determinista.** Un script resuelve qué grupos de secretos están completos:
   - Grupo completo: se exportan sus variables a `$GITHUB_ENV`.
   - Grupo vacío: se omite.
   - Grupo a medias: el job **falla** y nombra los secretos que faltan, nunca sus valores.
   - No se exportan variables vacías, porque no está verificado cómo trata Tauri una variable definida pero vacía.
   - En macOS, sin certificado, la firma es ad hoc (`"signingIdentity": "-"` en `tauri.macos.conf.json`). Con certificado, `APPLE_SIGNING_IDENTITY` sustituye ese valor.
   - En Windows, con certificado, el thumbprint se inyecta en el build con `tauri build --config <json temporal>`. Ningún dato de firma se versiona.
   - Notarización: solo con Apple ID (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`). La variante con clave de API de App Store Connect queda fuera de alcance.
6. **Release en borrador, creado una sola vez.**
   - Un job `create-release` valida el tag y crea el borrador (o reutiliza el borrador de ese tag si se relanza el workflow). Pasa `releaseId` a los jobs de build, para evitar que dos jobs de la matriz creen dos borradores.
   - Un job final sube `SHA256SUMS.txt`.
   - Publicar el borrador lo hace un humano.
7. **Ensayo en los PR.** El mismo workflow se ejecuta en los `pull_request` que tocan ficheros de empaquetado:
   - Construye sin firmar y **sin acceso a secretos de firma**.
   - Verifica los artefactos y los sube como artefactos del workflow.
   - Así se comprueba antes del merge, sin tags. Además, `workflow_dispatch` solo funciona con workflows que ya están en `main`.

## Criterios de aceptación

- **CA1** — Dada la configuración de Tauri del repo, cuando se ejecuta `node scripts/check-bundle.mjs`, entonces sale con 0. Dada una configuración con cualquiera de los defectos de la tabla de T1 (por ejemplo `identifier: "com.tauri.dev"`, un campo `version`, un icono que no existe o un `certificateThumbprint` versionado), cuando se valida, entonces sale con 1 y el mensaje nombra la regla incumplida.
- **CA2** — Dado `node scripts/quality-gate.mjs` con `src-tauri/tauri.conf.json` presente, cuando se ejecuta, entonces incluye el paso "Empaquetado (config Tauri)" y termina en verde.
- **CA3** — Dado el icono fuente `assets/icon/app-icon.svg`, cuando se comprueba `src-tauri/icons/`, entonces:
  - existen `32x32.png` (32×32), `128x128.png` (128×128), `128x128@2x.png` (256×256), `icon.ico` e `icon.icns`;
  - las dimensiones se leen de la cabecera PNG;
  - `bundle.icon` lista exactamente esos cinco ficheros.
- **CA4** — Dado un PR desde `chore/010-empaquetado` que toca ficheros de empaquetado, sin ningún secreto de firma, cuando se ejecuta `release.yml`, entonces:
  - el job de Windows produce exactamente un `.msi` y un `-setup.exe`;
  - el de macOS produce exactamente un `.dmg`;
  - `lipo -archs` sobre el ejecutable de la `.app` devuelve `x86_64 arm64`;
  - `codesign --verify --deep --strict` sale con 0 (firma ad hoc);
  - los tres instaladores quedan como artefactos del workflow;
  - no se crea ninguna release.
- **CA5** — Dado el tag `v<versión de src-tauri/Cargo.toml>` sin secretos de firma, cuando se empuja, entonces:
  - existe **un** borrador de release (no publicado) con ese tag;
  - contiene un `.msi`, un `-setup.exe`, un `.dmg` y `SHA256SUMS.txt`;
  - `shasum -a 256 -c SHA256SUMS.txt` en macOS y `sha256sum -c` (Git Bash) en Windows validan los tres instaladores.
- **CA6** — Dado un tag que no coincide con la versión (por ejemplo `v0.2.0` con `0.1.0`), o con formato no admitido (`v0.1.0-beta.1`, `v256.0.0`), cuando se empuja, entonces `create-release` falla con un mensaje que nombra la versión esperada, no se construye nada y no se crea ninguna release.
- **CA7** — Dado el entorno de un job, cuando se resuelve la firma con `node scripts/signing-env.mjs`, entonces se cumplen las tablas de T5:
  - sin secretos: modo `adhoc` (macOS) o `unsigned` (Windows), sin exportar variables;
  - con un grupo completo: se exportan solo las variables de ese grupo;
  - con un grupo a medias: sale con 1 y nombra los secretos que faltan, sin imprimir ningún valor.
- **CA8** — Dado `release.yml`, cuando se busca `rg -n "secrets\.(APPLE|WINDOWS)" .github/workflows/release.yml`, entonces todas las coincidencias están en pasos con `if: startsWith(github.ref, 'refs/tags/v')`. Un PR nunca recibe secretos de firma.
- **CA9** — Dado `docs/`, cuando se revisa, entonces:
  - existen `docs/empaquetado.md` (cómo publicar una versión, lista de secretos y cómo activar la firma), `docs/instalacion-sin-firma.md` (Gatekeeper en macOS 14 y 15+, SmartScreen y Smart App Control en Windows, verificación de SHA256) y `docs/checklist-prueba-mac.md`;
  - `rg -n "^## " docs/instalacion-sin-firma.md` devuelve al menos las secciones `macOS`, `Windows` y `Verificar la descarga`.
- **CA10** — Dado el plan aplicado, cuando se ejecuta `node scripts/quality-gate.mjs`, entonces termina en verde.

## Tareas

> Las tareas de código son **(developer)** con TDD: primero los tests de la tabla (en rojo) y después el código (en verde). **(O)** = orchestrator, con el usuario. Los nombres de los tests son normativos.
>
> Los scripts CLI (`scripts/*.mjs`) son envoltorios finos: leen ficheros, el entorno o la salida de `cargo`/`gh`, llaman a funciones puras de `scripts/lib/` y escriben el resultado. Solo se testean las funciones puras. Todas las rutas se construyen con `node:path` (R3).

### T1 — Validador puro de la configuración de empaquetado (developer)
- **Ficheros/módulos:** `scripts/lib/bundle-config.mjs` (nuevo), `scripts/lib/bundle-config.test.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** CA1, CA3 (lógica)
- **Subtareas:**
  - [ ] T1.1 `mergePatch(base, patch)`: JSON Merge Patch (RFC 7396). `null` en el parche borra la clave, los objetos se fusionan recursivamente y los arrays se sustituyen.
  - [ ] T1.2 `pngDimensions(buffer)`: lee ancho y alto del chunk IHDR (bytes 16–23, big-endian). Lanza un `Error` si la firma PNG no es `89 50 4E 47 0D 0A 1A 0A` o si el buffer mide menos de 24 bytes.
  - [ ] T1.3 `checkBundleConfig({ base, windows, macos }, { exists, readPng })` devuelve un array de errores `"[regla] mensaje"`. Está vacío si todo es correcto. `exists(relPath)` y `readPng(relPath)` se inyectan, sin fs real. Reglas:

    | Regla | Comprobación |
    |---|---|
    | `identificador` | `base.identifier` cumple `^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$`, no es `com.tauri.dev` y no acaba en `.app` |
    | `nombre` | `base.productName` existe y cumple `^[^/\\:*?"<>\|]+$` |
    | `version` | ninguno de los tres ficheros define `version` (la fuente única es Cargo) |
    | `activo` | `bundle.active === true` |
    | `targets` | `base.bundle.targets` no existe; la fusión de Windows da exactamente `{msi, nsis}` y la de macOS `{app, dmg}` (sin importar el orden ni las mayúsculas) |
    | `iconos` | `bundle.icon` es exactamente `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`, `icons/icon.ico`; todos existen; los PNG miden 32, 128 y 256 px de lado |
    | `categoria` | `bundle.category === "DeveloperTool"` |
    | `upgrade-code` | la fusión de Windows tiene `bundle.windows.wix.upgradeCode` con formato UUID (`8-4-4-4-12` hexadecimal) |
    | `firma-mac` | la fusión de macOS tiene `bundle.macOS.signingIdentity === "-"` y `minimumSystemVersion` con formato `^\d+\.\d+$` |
    | `sin-firma-versionada` | ninguno de los tres ficheros contiene `certificateThumbprint` ni `signCommand` |
    | `sin-updater` | `bundle.createUpdaterArtifacts` no existe o es `false` |
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `merge_patch_fusiona_objetos` | `{a:{b:1,c:2}}` + `{a:{c:3}}` | `{a:{b:1,c:3}}` |
  | `merge_patch_null_borra` | `{a:1,b:2}` + `{b:null}` | `{a:1}` |
  | `merge_patch_sustituye_arrays` | `{t:["x","y"]}` + `{t:["z"]}` | `{t:["z"]}` |
  | `png_dimensiones_validas` | buffer con firma PNG + IHDR 32×32 | `{width:32,height:32}` |
  | `png_firma_invalida` | 24 bytes que empiezan por `GIF8` | lanza `Error` |
  | `png_buffer_corto` | 10 bytes | lanza `Error` |
  | `config_valida_sin_errores` | fixture válido (base + windows + macos), `exists` → true, PNG con el tamaño correcto | `[]` |
  | `identificador_por_defecto_rechazado` | `identifier: "com.tauri.dev"` | un error que empieza por `[identificador]` |
  | `identificador_con_guion_bajo_rechazado` | `"io.github.x.app_llama"` | `[identificador]` |
  | `identificador_un_segmento_rechazado` | `"appllama"` | `[identificador]` |
  | `identificador_acabado_en_app_rechazado` | `"io.github.x.app"` | `[identificador]` |
  | `nombre_con_barra_rechazado` | `productName: "app/llama"` | `[nombre]` |
  | `version_definida_rechazada` | `macos.version = "0.1.0"` | `[version]` |
  | `bundle_inactivo_rechazado` | `bundle.active: false` | `[activo]` |
  | `targets_en_base_rechazados` | `base.bundle.targets: "all"` | `[targets]` |
  | `targets_windows_incompletos` | Windows con `["nsis"]` | `[targets]` |
  | `targets_mayusculas_aceptados` | Windows con `["MSI","nsis"]` | `[]` |
  | `icono_inexistente_rechazado` | `exists("icons/icon.icns")` → false | `[iconos]` con `icon.icns` en el mensaje |
  | `icono_tamano_incorrecto` | `128x128@2x.png` de 128×128 | `[iconos]` |
  | `icono_extra_rechazado` | `bundle.icon` con un sexto fichero | `[iconos]` |
  | `upgrade_code_invalido` | `upgradeCode: "1234"` | `[upgrade-code]` |
  | `upgrade_code_ausente` | sin `wix` | `[upgrade-code]` |
  | `firma_mac_identidad_real_rechazada` | `signingIdentity: "Developer ID Application: X"` | `[firma-mac]` |
  | `minimo_macos_invalido` | `minimumSystemVersion: "once"` | `[firma-mac]` |
  | `thumbprint_versionado_rechazado` | `windows.bundle.windows.certificateThumbprint: "ab"` | `[sin-firma-versionada]` |
  | `sign_command_versionado_rechazado` | `base.bundle.windows.signCommand: "x %1"` | `[sin-firma-versionada]` |
  | `updater_activado_rechazado` | `bundle.createUpdaterArtifacts: true` | `[sin-updater]` |
  | `varios_errores_acumulados` | `com.tauri.dev` + `version` | 2 errores, uno por regla |

### T2 — Icono fuente e iconos generados (developer)
- **Ficheros/módulos:** `assets/icon/app-icon.svg` (nuevo), `src-tauri/icons/*` (regenerados), `docs/empaquetado.md` (nuevo, solo la sección "Iconos")
- **Depende de:** —
- **Cubre:** CA3
- **Subtareas:**
  - [ ] T2.1 Crear `assets/icon/app-icon.svg`: cuadrado, fondo transparente y legible a 32 px. Si el usuario aporta un diseño (ver Preguntas abiertas), se usa el suyo. Si no, se usa un provisional geométrico (cuadrado redondeado con las iniciales "AL").
  - [ ] T2.2 Generar los iconos con el CLI de Tauri que instaló 003, por ejemplo `npm --prefix ui run tauri -- icon ../assets/icon/app-icon.svg --output ../src-tauri/icons`. El developer ajusta la invocación a la que use 003 y la documenta. Se versionan los ficheros generados. Se borran de `src-tauri/icons/` los que queden de la plantilla y no genere `tauri icon`.
  - [ ] T2.3 `docs/empaquetado.md` § "Iconos": la fuente, la orden exacta para regenerarlos y la regla de que nunca se editan a mano.
- **Tests unitarios:** ninguno nuevo (son activos binarios). **Verificación:** con T3 aplicado, `node scripts/check-bundle.mjs` comprueba existencia y tamaños (reglas `iconos` de T1). Además, revisión visual del `icon.ico` en el Explorador de Windows (fichero, no instalador).

### T3 — Configuración de Tauri, CLI de validación y paso del gate (developer)
- **Ficheros/módulos:** `src-tauri/tauri.conf.json`, `src-tauri/tauri.windows.conf.json` (nuevo), `src-tauri/tauri.macos.conf.json` (nuevo), `scripts/check-bundle.mjs` (nuevo), `scripts/quality-gate.mjs`, `docs/GOVERNANCE.md` (§2, tabla)
- **Depende de:** T1, T2
- **Cubre:** CA1, CA2
- **Subtareas:**
  - [ ] T3.1 `tauri.conf.json`:
    - se mantienen `productName` e `identifier` de 003 si cumplen T1; si no, se usan los valores que confirme el usuario (Preguntas abiertas);
    - se elimina `version`;
    - `bundle.active: true`;
    - `bundle.icon` con los cinco ficheros;
    - `bundle.category: "DeveloperTool"`;
    - `bundle.shortDescription`, `bundle.publisher` y `bundle.copyright` (sin año dinámico);
    - sin `bundle.targets`.
  - [ ] T3.2 `tauri.windows.conf.json`: `bundle.targets: ["msi","nsis"]` y `bundle.windows.wix.upgradeCode`.
    - Se genera **una vez** un UUID y se fija para siempre, porque si cambia se rompen las actualizaciones del `.msi`. La orden con la que se generó queda en `docs/empaquetado.md`.
    - `bundle.windows.nsis.installMode: "currentUser"`, explícito.
  - [ ] T3.3 `tauri.macos.conf.json`: `bundle.targets: ["app","dmg"]`, `bundle.macOS.signingIdentity: "-"` y `bundle.macOS.minimumSystemVersion` con el valor que confirme el usuario (propuesta: `"11.0"`).
  - [ ] T3.4 `scripts/check-bundle.mjs`:
    - lee los tres JSON de `src-tauri/` (si falta uno de los de plataforma, cuenta como `{}`);
    - inyecta `exists` y `readPng` con rutas relativas a `src-tauri/`;
    - imprime los errores y sale con 1 si hay alguno, o con 0 e imprime `config de empaquetado OK`;
    - un JSON inválido sale con 1 y nombra el fichero.
  - [ ] T3.5 `quality-gate.mjs` `buildSteps`: el paso `"Empaquetado (config Tauri)"` → `node scripts/check-bundle.mjs` se añade si existe `src-tauri/tauri.conf.json`.
  - [ ] T3.6 `docs/GOVERNANCE.md` §2: una fila más en la tabla del gate (`Empaquetado | node scripts/check-bundle.mjs | src-tauri/tauri.conf.json`).
  - [ ] T3.7 Comprobar en local que `npm --prefix ui run tauri -- build --bundles nsis` (Windows) genera `*-setup.exe` con el icono nuevo. Es una comprobación manual del developer, de la que deja constancia en su entrada de memoria.
- **Tests unitarios:** los de T1 cubren la lógica. **Verificación:**
  - `node scripts/check-bundle.mjs` sale con 0 sobre el repo.
  - Con una copia temporal de `tauri.conf.json` con `"identifier": "com.tauri.dev"`, sale con 1. Se restaura después.
  - `node scripts/quality-gate.mjs` muestra el paso nuevo en verde.

### T4 — Versión, tag y borrador de release: lógica pura (developer)
- **Ficheros/módulos:** `scripts/lib/release-version.mjs` (nuevo), `scripts/lib/release-version.test.mjs` (nuevo), `scripts/check-release-tag.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** CA5, CA6
- **Subtareas:**
  - [ ] T4.1 `appVersionFromMetadata(metadata, manifestSuffix)`: entre los `metadata.packages` (salida de `cargo metadata --no-deps --format-version 1`), busca el paquete cuyo `manifest_path`, con los separadores normalizados a `/`, acaba en `src-tauri/Cargo.toml`, y devuelve su `version`. Lanza un `Error` si no hay ninguno o si hay más de uno.
  - [ ] T4.2 `checkReleaseVersion(version)`: devuelve `null` si cumple `^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$` con major ≤ 255, minor ≤ 255 y patch ≤ 65535 (límites de MSI). Si no, devuelve un mensaje de error.
  - [ ] T4.3 `checkTag(tag, version)`: devuelve `null` si `tag === "v" + version` y la versión es válida. Si no, devuelve un mensaje que contiene `v<versión>` (el tag esperado).
  - [ ] T4.4 `pickDraftRelease(releases, tag)`: recibe la lista de la API de GitHub (`[{id, tag_name, draft}]`).
    - Devuelve `{ action: "reuse", id }` si hay un borrador con ese tag, o `{ action: "create" }` si no hay ninguna release con ese tag.
    - Lanza un `Error` si ya hay una release **publicada** con ese tag (no se sobrescribe) o más de un borrador.
  - [ ] T4.5 `scripts/check-release-tag.mjs <tag>`: ejecuta `cargo metadata --no-deps --format-version 1 --locked`, aplica T4.1 y T4.3, imprime `version=<v>` y sale con 0, o imprime el error y sale con 1.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `version_de_metadata_unix` | packages con `manifest_path: "/r/src-tauri/Cargo.toml"`, `version: "0.1.0"` | `"0.1.0"` |
  | `version_de_metadata_windows` | `manifest_path: "C:\\r\\src-tauri\\Cargo.toml"` | `"0.1.0"` |
  | `version_de_metadata_ignora_core` | packages `crates/core/Cargo.toml` (0.9.0) + `src-tauri/Cargo.toml` (0.1.0) | `"0.1.0"` |
  | `version_de_metadata_sin_app` | solo `crates/core/Cargo.toml` | lanza `Error` |
  | `version_valida` | `"0.1.0"`, `"255.255.65535"` | `null` |
  | `version_prerelease_rechazada` | `"0.1.0-beta.1"` | mensaje de error |
  | `version_major_excede_msi` | `"256.0.0"` | mensaje que menciona `255` |
  | `version_patch_excede_msi` | `"1.0.65536"` | mensaje que menciona `65535` |
  | `version_cero_inicial_rechazada` | `"01.0.0"` | mensaje de error |
  | `tag_coincide` | `("v0.1.0","0.1.0")` | `null` |
  | `tag_no_coincide` | `("v0.2.0","0.1.0")` | mensaje que contiene `v0.1.0` |
  | `tag_sin_v_rechazado` | `("0.1.0","0.1.0")` | mensaje que contiene `v0.1.0` |
  | `borrador_reutilizado` | `[{id:7,tag_name:"v0.1.0",draft:true}]`, `"v0.1.0"` | `{action:"reuse",id:7}` |
  | `borrador_creado_si_no_existe` | `[{id:1,tag_name:"v0.0.9",draft:false}]`, `"v0.1.0"` | `{action:"create"}` |
  | `release_publicada_no_se_sobrescribe` | `[{id:3,tag_name:"v0.1.0",draft:false}]` | lanza `Error` con el mensaje "ya publicada" |
  | `varios_borradores_rechazados` | dos borradores con `v0.1.0` | lanza `Error` |

### T5 — Resolución de la firma opcional (developer)
- **Ficheros/módulos:** `scripts/lib/signing-env.mjs` (nuevo), `scripts/lib/signing-env.test.mjs` (nuevo), `scripts/signing-env.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** CA7
- **Subtareas:**
  - [ ] T5.1 `resolveSigning(env, platform)` con `platform ∈ {"macos","windows"}` devuelve `{ mode, notarize, exports, errors }`. Un valor cuenta como presente si no es `undefined` y no queda vacío tras `trim()`.
    - **macOS**, grupo de firma `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` y `APPLE_SIGNING_IDENTITY`:
      - los tres presentes: `mode: "signed"` y se exportan;
      - ninguno: `mode: "adhoc"` y no se exporta nada;
      - entre uno y dos: error.
    - **macOS**, grupo de notarización `APPLE_ID`, `APPLE_PASSWORD` y `APPLE_TEAM_ID`:
      - los tres presentes y firma `signed`: `notarize: true` y se exportan;
      - los tres presentes sin firma: error ("la notarización requiere certificado Developer ID");
      - entre uno y dos: error.
    - **Windows**, grupo `WINDOWS_CERTIFICATE` y `WINDOWS_CERTIFICATE_PASSWORD`:
      - los dos presentes: `mode: "signed"`; no se exporta nada, porque el paso de importación los lee directamente;
      - ninguno: `mode: "unsigned"`;
      - uno solo: error.
    - Un valor con `\n` o `\r` → error, para evitar la inyección en `$GITHUB_ENV`.
    - Los mensajes de error **nunca** incluyen valores, solo nombres de variables.
  - [ ] T5.2 `formatGithubEnv(exports)` → líneas `NOMBRE=valor` ordenadas por nombre.
  - [ ] T5.3 `windowsSigningConfig(thumbprint, timestampUrl)` → objeto `{bundle:{windows:{certificateThumbprint, digestAlgorithm:"sha256", timestampUrl}}}`.
    - El thumbprint debe tener 40 caracteres hexadecimales (se normaliza a mayúsculas y sin espacios). Si no, lanza un `Error`.
    - `timestampUrl` debe empezar por `http://` o `https://`. Si no se indica, vale `http://timestamp.digicert.com`.
  - [ ] T5.4 CLI `scripts/signing-env.mjs`:
    - `--platform macos|windows`: escribe en el fichero `$GITHUB_ENV` las líneas de T5.2 y en `$GITHUB_OUTPUT` `mode=<m>` y `notarize=<bool>`;
    - imprime solo el modo y los nombres exportados;
    - si hay errores, sale con 1;
    - `--windows-config <thumbprint> <salida.json>`: escribe el JSON de T5.3; lee `WINDOWS_TIMESTAMP_URL` del entorno.
- **Tests unitarios** (los fixtures usan valores falsos y cortos, por ejemplo `"cert-falso"`):
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `mac_sin_secretos_adhoc` | `{}`, `"macos"` | `mode:"adhoc"`, `notarize:false`, `exports:{}`, `errors:[]` |
  | `mac_vacios_cuentan_como_ausentes` | los 6 `APPLE_*` = `"  "` | `mode:"adhoc"`, sin errores |
  | `mac_firma_completa` | 3 de firma | `mode:"signed"`, `exports` con exactamente esas 3 claves |
  | `mac_firma_incompleta_error` | solo `APPLE_CERTIFICATE` | `errors` nombra `APPLE_CERTIFICATE_PASSWORD` y `APPLE_SIGNING_IDENTITY` |
  | `mac_firma_y_notarizacion` | 3 de firma + 3 de notarización | `notarize:true`, 6 claves exportadas |
  | `mac_notarizacion_sin_firma_error` | solo los 3 de notarización | error que menciona `Developer ID` |
  | `mac_notarizacion_incompleta_error` | 3 de firma + `APPLE_ID` | error que nombra `APPLE_PASSWORD` y `APPLE_TEAM_ID` |
  | `error_no_revela_valores` | `APPLE_CERTIFICATE: "cert-falso"` (incompleto) | ningún error contiene `cert-falso` |
  | `valor_multilinea_rechazado` | grupo completo con `APPLE_SIGNING_IDENTITY: "a\nB=c"` | error que nombra `APPLE_SIGNING_IDENTITY` |
  | `windows_sin_secretos_unsigned` | `{}`, `"windows"` | `mode:"unsigned"`, sin errores |
  | `windows_completo_signed` | los 2 `WINDOWS_*` | `mode:"signed"`, `exports:{}` |
  | `windows_incompleto_error` | solo `WINDOWS_CERTIFICATE` | error que nombra `WINDOWS_CERTIFICATE_PASSWORD` |
  | `windows_ignora_secretos_apple` | 3 de firma Apple, `"windows"` | `mode:"unsigned"`, `exports:{}` |
  | `plataforma_desconocida_lanza` | `{}`, `"linux"` | lanza `Error` |
  | `github_env_ordenado` | `{B:"2",A:"1"}` | `"A=1\nB=2"` |
  | `config_windows_valida` | thumbprint de 40 hex en minúsculas con espacios, sin URL | `certificateThumbprint` en mayúsculas sin espacios, `digestAlgorithm:"sha256"`, URL de DigiCert |
  | `config_windows_thumbprint_invalido` | `"abc"` | lanza `Error` |
  | `config_windows_url_invalida` | URL `"ftp://x"` | lanza `Error` |

### T6 — Verificación de artefactos y SHA256SUMS (developer)
- **Ficheros/módulos:** `scripts/lib/release-artifacts.mjs` (nuevo), `scripts/lib/release-artifacts.test.mjs` (nuevo), `scripts/release-artifacts.mjs` (nuevo)
- **Depende de:** —
- **Cubre:** CA4, CA5
- **Subtareas:**
  - [ ] T6.1 `selectInstallers(paths, platform)`: de `artifactPaths` (array JSON de tauri-action) devuelve los instaladores.
    - Windows: exactamente un `*.msi` y un `*-setup.exe`.
    - macOS: exactamente un `*.dmg`.
    - Ignora el resto (`.app`, `.sig`, `.tar.gz`). Lanza un `Error` si falta alguno o si hay duplicados. Compara por `basename`, sea cual sea el separador.
  - [ ] T6.2 `formatSha256Sums(entries)`: recibe `[{name, sha256}]` y devuelve líneas `<sha256>  <name>` (dos espacios) ordenadas por `name`, con `\n` final. Es compatible con `sha256sum -c` y `shasum -a 256 -c`. Lanza un `Error` si un hash no tiene 64 caracteres hexadecimales o si hay nombres repetidos.
  - [ ] T6.3 `sha256File(path)`: hash en streaming con `node:crypto`.
  - [ ] T6.4 CLI `scripts/release-artifacts.mjs`:
    - `select --platform <p> --paths '<json>' --out <dir>`: copia los instaladores a `<dir>` y escribe `<dir>/SHA256SUMS-<p>.txt`;
    - `merge <dir>`: une todos los `SHA256SUMS-*.txt` de `<dir>` en `SHA256SUMS.txt` con T6.2.
- **Tests unitarios:**
  | Test | Entrada | Resultado esperado |
  |---|---|---|
  | `instaladores_windows` | `["…\\msi\\App_0.1.0_x64_en-US.msi","…\\nsis\\App_0.1.0_x64-setup.exe","…\\App.exe"]` | los dos primeros |
  | `instaladores_macos` | `[".../dmg/App_0.1.0_universal.dmg",".../macos/App.app"]` | solo el `.dmg` |
  | `falta_msi_error` | solo `-setup.exe` | lanza `Error` que menciona `.msi` |
  | `dmg_duplicado_error` | dos `.dmg` | lanza `Error` |
  | `ignora_firmas_updater` | `.msi`, `-setup.exe`, `.msi.sig` | `.msi` y `-setup.exe` |
  | `sumas_ordenadas_formato` | `[{name:"b.dmg",sha256:"a"×64},{name:"a.msi",sha256:"b"×64}]` | `"bbb…  a.msi\naaa…  b.dmg\n"` |
  | `suma_hash_invalido` | `sha256:"xyz"` | lanza `Error` |
  | `suma_nombre_repetido` | dos entradas `a.msi` | lanza `Error` |
  | `sha256_fichero_conocido` | fichero temporal (`mkdtemp`) con `"abc"` | `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |
  | `sha256_fichero_vacio` | fichero temporal vacío | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

### T7 — Workflow `release.yml`: ensayo sin firma en los PR (developer)
- **Ficheros/módulos:** `.github/workflows/release.yml` (nuevo)
- **Depende de:** T3, T6
- **Cubre:** CA4, CA8
- **Subtareas:**
  - [ ] T7.1 Disparadores:
    - `pull_request` con `paths`: `.github/workflows/release.yml`, `src-tauri/tauri*.conf.json`, `src-tauri/icons/**`, `assets/icon/**`, `scripts/check-bundle.mjs`, `scripts/signing-env.mjs`, `scripts/release-artifacts.mjs`, `scripts/check-release-tag.mjs` y `scripts/lib/{bundle-config,signing-env,release-artifacts,release-version}.mjs`;
    - `push.tags: ["v*"]`;
    - `concurrency: release-${{ github.ref }}`, sin `cancel-in-progress`;
    - `permissions: contents: read` a nivel de workflow.
  - [ ] T7.2 Job `build`, con matriz `include`:
    - `{os: windows-latest, platform: windows, args: ""}`;
    - `{os: macos-latest, platform: macos, args: "--target universal-apple-darwin"}`;
    - `fail-fast: false`.
    - Pasos: checkout@v4, setup-node@v4 (24), toolchain según `rust-toolchain.toml` (igual que `ci.yml`), y en macOS `rustup target add aarch64-apple-darwin x86_64-apple-darwin`. Después `Swatinem/rust-cache@v2`, `npm ci --prefix ui` y `node scripts/check-bundle.mjs`.
  - [ ] T7.3 Build con `tauri-apps/tauri-action@v1`, sin `releaseId` ni `tagName` en los PR:
    - `args: ${{ matrix.args }}`;
    - `uploadUpdaterJson: false`;
    - `projectPath` y `tauriScript` coherentes con la invocación del CLI de 003 (el developer los toma de su workflow de build y los anota en `docs/empaquetado.md`).
  - [ ] T7.4 Verificación:
    - `node scripts/release-artifacts.mjs select --platform ${{ matrix.platform }} --paths '${{ steps.tauri.outputs.artifactPaths }}' --out "$RUNNER_TEMP/dist"`;
    - en macOS, `lipo -archs` sobre `…/universal-apple-darwin/release/bundle/macos/*.app/Contents/MacOS/*`, que debe contener `x86_64` y `arm64`, y `codesign --verify --deep --strict --verbose=2` sobre la `.app`;
    - el `codesign -dv` se imprime en el log (`Signature=adhoc` esperado en los PR).
  - [ ] T7.5 `actions/upload-artifact@v4` con `name: instaladores-${{ matrix.platform }}` y `$RUNNER_TEMP/dist`.
  - [ ] T7.6 Ningún paso de este job referencia `secrets.APPLE_*` ni `secrets.WINDOWS_*` (los añade T9 con `if` de tag).
- **Tests unitarios:** no aplica (YAML). **Verificación:**
  - la lógica que invoca está cubierta por T3 y T6;
  - CA4 se comprueba en vivo en T11.1;
  - `rg -n "secrets\.(APPLE|WINDOWS)" .github/workflows/release.yml` no devuelve nada tras T7.

### T8 — Workflow: borrador de release por tag y SHA256SUMS (developer)
- **Ficheros/módulos:** `.github/workflows/release.yml`
- **Depende de:** T4, T7
- **Cubre:** CA5, CA6
- **Subtareas:**
  - [ ] T8.1 Job `create-release`:
    - `if: startsWith(github.ref, 'refs/tags/v')`, `runs-on: ubuntu-latest` (solo scripting: no construye para Linux), `permissions: contents: write`;
    - pasos: checkout, setup-node, toolchain y `node scripts/check-release-tag.mjs "$GITHUB_REF_NAME"`;
    - después lista las releases con `gh api repos/$GITHUB_REPOSITORY/releases --paginate` y decide con `pickDraftRelease`, a través de un subcomando `pick-draft` de `check-release-tag.mjs` que lee el JSON por stdin;
    - si toca crear, `gh api -X POST repos/$GITHUB_REPOSITORY/releases -f tag_name=… -f name="app-llama $GITHUB_REF_NAME" -F draft=true -f body="Instaladores sin firmar salvo que se indique. Ver docs/instalacion-sin-firma.md."`;
    - salida `release_id`.
  - [ ] T8.2 Job `build`:
    - `needs: create-release` solo en tags. Se usa `if: always() && (needs.create-release.result == 'success' || needs.create-release.result == 'skipped')`, y en los PR `create-release` queda en `skipped`;
    - en tags, `permissions: contents: write` y `releaseId: ${{ needs.create-release.outputs.release_id }}`;
    - si `create-release` falla, `build` no corre (CA6).
  - [ ] T8.3 Job `checksums`:
    - `needs: build`, solo en tags, en `ubuntu-latest`;
    - descarga los artefactos `instaladores-*` (`actions/download-artifact@v4`, `merge-multiple: true`);
    - ejecuta `node scripts/release-artifacts.mjs merge`;
    - sube el fichero con `gh release upload "$GITHUB_REF_NAME" SHA256SUMS.txt --clobber`.
  - [ ] T8.4 La release **nunca** se publica desde el workflow: nada de `releaseDraft: false` ni `gh release edit --draft=false`.
- **Tests unitarios:** la lógica está en T4 (`pickDraftRelease`, `checkTag`) y T6 (`formatSha256Sums`). **Verificación:** en vivo en T11.2 y T11.3.

### T9 — Workflow: firma opcional en tags (developer)
- **Ficheros/módulos:** `.github/workflows/release.yml`, `docs/empaquetado.md` (sección "Firma")
- **Depende de:** T5, T8
- **Cubre:** CA7, CA8
- **Subtareas:**
  - [ ] T9.1 Paso "Resolver firma", con `if: startsWith(github.ref, 'refs/tags/v')` y `id: signing`:
    - `env` con los `secrets.APPLE_*` (6) y `secrets.WINDOWS_*` (2) solo en este paso;
    - ejecuta `node scripts/signing-env.mjs --platform ${{ matrix.platform }}`;
    - si falla, el job falla.
  - [ ] T9.2 Paso "Importar certificado de Windows":
    - `if: startsWith(github.ref, 'refs/tags/v') && matrix.platform == 'windows' && steps.signing.outputs.mode == 'signed'`;
    - en PowerShell, decodifica `WINDOWS_CERTIFICATE` a `$env:RUNNER_TEMP\cert.pfx` e importa con `Import-PfxCertificate -CertStoreLocation Cert:\CurrentUser\My`;
    - borra el `.pfx`;
    - ejecuta `node scripts/signing-env.mjs --windows-config <thumbprint> $env:RUNNER_TEMP\signing.json`;
    - añade `--config <ruta>` a la salida `extra_args`;
    - `WINDOWS_TIMESTAMP_URL` es una variable de repositorio (`vars.`), no un secreto.
  - [ ] T9.3 El `args` de tauri-action pasa a `${{ matrix.args }} ${{ steps.windows-cert.outputs.extra_args }}`. Las variables `APPLE_*` llegan a Tauri por `$GITHUB_ENV`, y solo si el grupo estaba completo.
  - [ ] T9.4 En macOS con `mode == 'signed'`, el log debe mostrar `Authority=Developer ID Application` (`codesign -dv`). Con `notarize == 'true'`, además, `spctl -a -vv -t install` sobre el `.dmg` → `accepted` y `source=Notarized Developer ID`. Estos pasos solo corren en ese modo.
  - [ ] T9.5 `docs/empaquetado.md` § "Firma":
    - tabla de secretos (nombre, plataforma, grupo, cómo obtenerlo);
    - regla de grupos completos;
    - qué produce cada combinación;
    - aviso de que la ruta firmada no se ha podido probar sin certificados.
- **Tests unitarios:** los de T5. **Verificación:**
  - el `rg` de CA8 solo devuelve líneas del paso T9.1;
  - en el ensayo de T11.1 (PR) los pasos de firma salen como `skipped`;
  - en T11.2 (tag sin secretos), "Resolver firma" imprime `mode=adhoc` en macOS y `mode=unsigned` en Windows.

### T10 — Documentación de instalación y checklist de Mac (developer)
- **Ficheros/módulos:** `docs/instalacion-sin-firma.md` (nuevo), `docs/checklist-prueba-mac.md` (nuevo), `docs/empaquetado.md` (sección "Publicar una versión")
- **Depende de:** T8, T9
- **Cubre:** CA9
- **Subtareas:**
  - [ ] T10.1 `docs/instalacion-sin-firma.md`, con las secciones `## macOS`, `## Windows` y `## Verificar la descarga`:
    - macOS 14 o anterior: clic derecho > Abrir;
    - macOS 15 o posterior: primer intento, luego Ajustes del Sistema > Privacidad y seguridad > "Abrir igualmente";
    - alternativa por terminal: `xattr -dr com.apple.quarantine "/Applications/<productName>.app"`, con la advertencia de que solo se haga con descargas verificadas;
    - mensaje "está dañada" y qué significa;
    - Windows: SmartScreen ("Más información" > "Ejecutar de todas formas"); Smart App Control de Windows 11 puede bloquear un instalador sin firma sin ofrecer omitirlo, y se explican las consecuencias de desactivarlo; diferencia entre `.msi` (requiere administrador) y `-setup.exe` (usuario actual);
    - verificación con `shasum -a 256 -c SHA256SUMS.txt --ignore-missing` y `Get-FileHash`.
  - [ ] T10.2 `docs/checklist-prueba-mac.md`: lista de casillas con el resultado esperado de cada paso, más un apartado para anotar el modelo de Mac, la versión de macOS y la versión de la app:
    1. Descargar el `.dmg` de la release y verificar el SHA256.
    2. Montarlo y arrastrar a Aplicaciones.
    3. Primer arranque según `instalacion-sin-firma.md`.
    4. `lipo -archs …/Contents/MacOS/*` → `x86_64 arm64`.
    5. `codesign -dv` → `adhoc` o `Developer ID`.
    6. Monitor de Actividad → Tipo "Apple" en Apple Silicon (no "Intel").
    7. Icono correcto en el Dock y en Finder, sin el icono por defecto de Tauri.
    8. Acerca de / título con la versión del tag.
    9. Fase 1: detecta Metal y descarga llama.cpp; el binario arranca sin aviso de Gatekeeper (atributo de cuarentena retirado).
    10. Fase 2: buscar un modelo pequeño (por ejemplo, menos de 1 GB).
    11. Fase 3: descargar, pausar y reanudar.
    12. Fase 4: aparece en la Biblioteca con sus metadatos.
    13. Fase 5: iniciar el servidor, `/health` OK y `curl http://127.0.0.1:<puerto>/v1/models`.
    14. Fase 6: chat con respuesta en streaming.
    15. Salir de la app → `pgrep llama-server` no devuelve nada.
    16. Desinstalar: borrar la app y el directorio de datos (ruta documentada) y comprobar que no queda nada.
    17. Repetir los pasos 3–6 en un Mac Intel, si hay uno disponible.
  - [ ] T10.3 `docs/empaquetado.md` § "Publicar una versión":
    - subir la versión en `src-tauri/Cargo.toml` y en `Cargo.lock` (por PR);
    - tras el merge, `git tag vX.Y.Z` sobre `main` y `git push origin vX.Y.Z`;
    - revisar el borrador, pasar la checklist de Mac y publicar a mano;
    - cómo relanzar un tag fallido (borrar borrador y tag, o relanzar el workflow, que reutiliza el borrador).
- **Tests unitarios:** no aplica (documentación). **Verificación:**
  - el `rg` de CA9;
  - el reviewer comprueba que cada orden de la doc coincide con `release.yml` y con los scripts (nombres de ficheros, secretos y subcomandos).

### T11 — Verificación en vivo (O, con el usuario)
- **Depende de:** T1–T10
- **Cubre:** CA4, CA5, CA6, CA10
- **Subtareas:**
  - [ ] T11.1 Con el OK del usuario, push de `chore/010-empaquetado` y apertura del PR. `release.yml` corre en modo ensayo. Comprobar CA4 en el log y descargar `instaladores-windows` e `instaladores-macos`.
  - [ ] T11.2 Con el OK del usuario, tag `v<versión actual>` sobre el último commit de la rama (o sobre `main` tras el merge, según decida el usuario; ver Preguntas abiertas). Comprobar CA5 y descargar el borrador. En macOS: `shasum -a 256 -c SHA256SUMS.txt`.
  - [ ] T11.3 Tag de prueba que no coincide (por ejemplo `v0.0.0-x`) → comprobar CA6. Borrar después ese tag y, si es de prueba, el borrador de T11.2 (`gh release delete <tag> --cleanup-tag`).
  - [ ] T11.4 El usuario instala el `-setup.exe` en Windows y sigue `docs/instalacion-sin-firma.md`. Si dispone de un Mac, pasa `docs/checklist-prueba-mac.md` y anota el resultado en el PR.
  - [ ] T11.5 `node scripts/quality-gate.mjs` en verde (CA10).

## Fuera de alcance

- Linux (`.deb`, `.AppImage`, `.rpm`) y auto-update (`createUpdaterArtifacts`, `latest.json`): fuera del MVP (MVP §6).
- Comprar certificados (Apple Developer, OV/EV de Windows) y probar la ruta firmada con certificados reales. Queda preparada, pero sin probar.
- Notarización con clave de API de App Store Connect (`APPLE_API_*`), Azure Trusted Signing o Key Vault (`signCommand`).
- Mac App Store, Microsoft Store, sandbox de macOS (`com.apple.security.app-sandbox`) y entitlements propios.
- Fondo y diseño personalizados del `.dmg` (en CI no se aplican, tauri#1731).
- Versiones prerelease (`-beta.N`): incompatibles con MSI sin una `wix.version` aparte.
- Instalador offline de WebView2 (`offlineInstaller` o `fixedRuntime`).
- Cambiar `ci.yml` o el build de CI del plan 003.
- Generar notas de release automáticas.

## Riesgos

| Riesgo | Mitigación |
|---|---|
| `APPLE_SIGNING_IDENTITY` del entorno no sustituye al `"-"` de `tauri.macos.conf.json` | T9.4 comprueba `Authority=Developer ID Application` en el primer build firmado. Si falla, el plan alternativo es que `signing-env.mjs` genere un `--config` con la identidad, como ya se hace en Windows |
| Tauri trata una variable definida pero vacía como presente e intenta firmar | No se exportan variables vacías: solo los grupos completos, a través de `$GITHUB_ENV` (T5) |
| Dos jobs de la matriz crean dos borradores para el mismo tag | `create-release` crea o reutiliza el borrador una sola vez y pasa `releaseId` (T4.4, T8.1) |
| `gh release upload` no encuentra un borrador por tag | Si falla en T11.2, se sube con `gh api` a `uploads.github.com` usando `release_id`. Se decide en la revisión de T8 con el log real |
| Gatekeeper en macOS 15+ hace más difícil abrir apps sin notarizar | Documentado paso a paso (T10.1); la notarización queda lista para cuando haya cuenta de Apple Developer |
| Smart App Control bloquea el instalador sin firma en Windows 11 | Documentado (T10.1); la firma de Windows está lista para activarse con secretos |
| `upgradeCode` cambiado por error rompe las actualizaciones del `.msi` | Regla `upgrade-code` en el gate (T1) y documentación de que es inmutable (T3.2) |
| El build universal duplica el tiempo del job de macOS | `rust-cache`, y el ensayo solo corre en los PR que tocan ficheros de empaquetado |
| Una release de llama.cpp exige una versión de macOS mayor que `minimumSystemVersion` | Se alinea con el mínimo que use el plan de runtime (Preguntas abiertas) y se comprueba en la checklist (paso 9) |
| Hardened runtime + notarización impiden lanzar `llama-server` descargado | `llama-server` es un ejecutable aparte, no código cargado en la app. La cuarentena la retira la Fase 1. Se comprueba en el paso 9 de la checklist con un build firmado cuando exista |
| Un PR de un fork intenta obtener secretos | Los pasos con secretos solo corren en tags (CA8) y GitHub no pasa secretos a los forks |
| `tauri-action@v1` cambia su comportamiento en una versión menor | Se fija el tag mayor como indica la doc oficial. Si da problemas, se fija por SHA |

## Preguntas abiertas

1. **Identificador y nombre visible.** Si 003 no los fijó ya, se proponen `identifier: "io.github.devlitus.app-llama"` y `productName: "app-llama"`. El identificador **no debe cambiar** después de la primera release: Tauri lo usa para el directorio de datos y el llavero. ¿Se confirman?
2. **Icono.** ¿Hay un diseño de icono (SVG o PNG de 1024×1024 con transparencia) o se usa el provisional geométrico de T2.1?
3. **`minimumSystemVersion` de macOS.** Se propone `"11.0"` (primer macOS con Apple Silicon). ¿Coincide con el mínimo de las releases de llama.cpp que use el plan de runtime?
4. **Mac de pruebas** (MVP §7.4). ¿Hay un Mac disponible para la checklist? ¿Apple Silicon, Intel o los dos? Sin Mac, CA4/CA5 se verifican solo en la CI y la checklist queda pendiente.
5. **Tag de prueba en T11.2.** ¿Se prueba el flujo de tag sobre la rama (y se borran después tag y borrador) o después del merge, con la primera versión real `v0.1.0` sobre `main`?
6. **Cuentas de firma.** ¿Hay intención de dar de alta Apple Developer (99 $/año) o comprar un certificado de Windows durante el MVP? Afecta a si la ruta firmada se prueba en este plan o en uno posterior.
7. **Publicador y copyright.** ¿Qué texto va en `bundle.publisher` (aparece como fabricante del `.msi`) y en `bundle.copyright`?
