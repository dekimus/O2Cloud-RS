# Revisión de seguridad

Fecha: 2026-10-07. Alcance: todo el código del cliente (`src/`, incluidos los
módulos opcionales `gui` y `webview`). Método: revisión manual del código crítico
(transporte, sesión, secretos, descarga/subida, salida, GUI) + comprobaciones
automatizadas (`unsafe`, shell, TLS, `unwrap`, dependencias).

## Resumen

Se encontraron **8 problemas** (1 alto, 3 medios, 4 bajos) y se corrigieron todos
en esta revisión. El resto de comprobaciones pasaron correctamente. No hay código
`unsafe`, no se usa shell ni `eval`, y TLS se verifica siempre.

## Hallazgos y correcciones

### S-1 — SSRF / fuga de la cookie de sesión en la descarga (ALTO) — CORREGIDO

El detalle de un archivo incluye un campo `url`. `absolutize` devolvía sin tocar
las URLs **absolutas**, y `download_signed_url` adjuntaba la cookie de sesión
(`Cookie: JSESSIONID=…; validationKey=…`) a *cualquier* URL. Un `url` apuntando a
un host ajeno (servidor comprometido, respuesta manipulada) habría provocado el
**envío de la cookie a un tercero** (SSRF + robo de sesión).

Corrección (`src/api/client.rs`): antes de descargar se exige que la URL resuelta
tenga **el mismo host y esquema** que la URL base configurada; si no, se rechaza
con `Error::Protocol`. Test: `download_rejects_foreign_signed_url`.

### S-2 — URLs firmadas (`k=…`) no se redactaban (MEDIO) — CORREGIDO

`redact_secrets` solo cubría `validationkey` y `jsessionid`. La URL de descarga
firmada lleva `&k=<firma>`; ante un error de transporte, `reqwest` incluye la URL
completa en el mensaje, con lo que **la firma podía filtrarse** en logs/errores.

Corrección (`src/util.rs`): se amplía la lista de claves sensibles (`k`,
`signature`, `token`, `access_token`, `accesstoken`, `refreshtoken`,
`encryption-token`, `pat`, …) y se exige **límite de clave** (inicio o `?`/`&`/`;`/
espacio/…), de modo que `link=` no se enmascara por contener una `k`. Tests:
`redacts_signed_download_signature`, `does_not_redact_key_inside_words`.

### S-3 — `Session` con `Debug` derivado (MEDIO) — CORREGIDO

`#[derive(Debug)]` en `Session` exponía `cookie` y `validation_key` si el valor
se imprimía con `{:?}` (p. ej. en un log de depuración). Corrección
(`src/auth/session.rs`): implementación manual de `Debug` que los redacta.

### S-4 — `MediaItem` con `Debug` derivado (BAJO-MEDIO) — CORREGIDO

`Debug` incluía el campo `url` (URL firmada). Corrección (`src/api/models.rs`):
`Debug` manual que sustituye `url` por `has_url: bool`.

### S-5 — `MemoryStore` con `Debug` derivado (BAJO) — CORREGIDO

`Debug` imprimía la cookie almacenada. Corrección (`src/auth/store.rs`): `Debug`
manual con `has_cookie: bool`.

### S-6 — Posible inyección de argumentos al abrir el navegador (BAJO) — CORREGIDO

`SystemBrowserOpener` ejecutaba `xdg-open/open/explorer <url>` sin shell, pero una
URL que empezara por `-` podría interpretarse como opción del lanzador.
Corrección (`src/auth/browser.rs`): se valida que la URL sea `http(s)://` antes de
pasarla al lanzador.

### S-7 — La cookie se mostraba en claro en la GUI (BAJO) — CORREGIDO

El campo de importación de cookie era multilínea visible. Corrección
(`src/gui/app.rs`): campo de **contraseña** (enmascarado).

### S-8 — SSRF vía redirección (BAJO) — MITIGADO

Aunque la URL inicial se valida por host (S-1), una respuesta `3xx` con
`Location` a otro host habría provocado una petición saliente a un host ajeno.
Corrección (`src/api/client.rs`): política de redirecciones que solo sigue al
**mismo host** y limita a 10 saltos. (Reqwest ya elimina `Cookie`/`Authorization`
en redirecciones cross-host.)

## Dependencias (`cargo audit`)

Ejecutado con `cargo-audit 0.22.2`: **sin vulnerabilidades** en el árbol (629
crates). Cuatro avisos informativos:

| Crate | Tipo | Nota |
| ----- | ---- | ---- |
| `glib 0.18.5` | **unsound** (RUSTSEC-2024-0429) | vía `gtk` del feature **`webview`**; corregido solo en `>=0.20.0` (wry 0.57 aún usa gtk 0.18). Afecta a `glib::VariantStrIter` (no usado directamente); riesgo práctico bajo. Evitable no usando `webview`. |
| `number_prefix 0.4.0` | unmaintained | transitivo de `indicatif` (CLI). |
| `proc-macro-error 1.0.4` | unmaintained | de compilación (macros). |
| `ttf-parser 0.25.1` | unmaintained | transitivo de `egui`/`eframe` (fuentes, GUI). |

Recomendación: ejecutar `cargo audit` en CI y actualizar `wry`/`gtk-rs` cuando
publiquen versiones con `glib >= 0.20`.

## Aspectos verificados (correctos)

- **TLS siempre verificado**: `reqwest` con `rustls-tls` (raíces Mozilla), sin
  `danger_accept_invalid_certs` ni equivalentes.
- **Sin `unsafe`** en todo el código.
- **Sin shell ni `eval`**: el único `std::process::Command` es lanzar el navegador
  (ejecución directa, sin intérprete de shell). Ver S-6.
- **Secretos**: solo en el keyring del sistema (o `O2CLOUD_COOKIE` para
  testing/automatización). No se escriben en config normal ni en texto plano. La
  sesión guardada es la **cabecera normalizada** (`save_session`).
- **Redacción**: los errores de transporte pasan por `redact_secrets`
  (`From<reqwest::Error>`); los logs solo registran `method` + `path` (nunca la
  query ni cabeceras).
- **Salida**: `MediaItem.url` tiene `skip_serializing` (nunca sale en JSON); las
  URLs firmadas no se imprimen.
- **Descarga**: saneado de nombres del servidor (sin `/`, `\`, `..`, NUL) para
  evitar *path traversal*; escritura en `<destino>.part` + `rename` atómico; el
  parcial se elimina ante error. Test: `sanitizes_traversal_and_empty`.
- **Subida/descarga en streaming**: nunca se carga el archivo entero en memoria.
- **Modelo de errores**: distingue HTTP / SAPI (en 200) / auth / local; un 401 se
  comunica como sesión caducada sin convertirlo todo en “auth failed”.
- **GUI**: el worker carga la sesión del keyring por operación; no persiste
  secretos en otros sitios; la cookie importada va al keyring.

## Riesgos residuales / recomendaciones

- **S-9 (informativo)**: `o2cloud login --import-cookie '<cookie>'` pasa el secreto
  como **argumento de línea de comandos**, visible en `ps` y en el historial del
  shell. Recomendación: usar el login por **WebView**/navegador de la GUI o
  `O2CLOUD_COOKIE` (evitando historial); el comando es un *fallback* de desarrollo.
- **S-10 (informativo)**: dependencias (ver tabla `cargo audit` arriba); ejecutar
  `cargo audit` en CI.
- **S-11 (informativo)**: las respuestas de metadatos se leen completas en memoria
  (`resp.text()`) sin límite de tamaño. Riesgo bajo (host de confianza); se podría
  añadir un tope si se desea.
- `webview`: `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` se documenta como
  *workaround* opcional y está etiquetado como **menos seguro**; el cliente no lo
  activa por defecto.

## Cobertura de tests relacionada

- Redacción de secretos (query, cookie, firma `k=`, sin falsos positivos).
- Rechazo de URL firmada a host ajeno (SSRF).
- Saneado de nombres (path traversal).
- Parseo/validación de la cookie de sesión (`validationKey` 32-hex, `JSESSIONID`).
- Mapeo de errores HTTP/SAPI (401/403/404/429/500, `QUO-*`, `SEC-*`).
