# Arquitectura del cliente O2 Cloud (Rust)

## 1. Objetivo

Cliente nativo Linux para O2 Cloud (Telefónica España), escrito en Rust.
Primera entrega: **CLI + librería reutilizable**. La capa de dominio debe
permitir añadir después TUI, FUSE/rclone o GUI sin tocar el cliente HTTP.

## 2. Capas

```
                        ┌─────────────────────────┐
                        │  CLI (clap)             │  args, confirmaciones,
                        │  - human / --json       │  códigos de salida
                        └───────────┬─────────────┘
                                    ▼
                        ┌─────────────────────────┐
                        │  Application / Services │  casos de uso:
                        │  status/quota/ls/stat   │  orquestación, sin HTTP
                        └───────────┬─────────────┘
                                    ▼
                        ┌─────────────────────────┐
                        │  O2 Cloud API client    │  transporte + envelope +
                        │  (SAPI)                 │  autenticación + modelos
                        └───────────┬─────────────┘
                                    ▼
        ┌──────────────────────────────────────────────────┐
        │ Local infrastructure                              │
        │  Auth (session/keyring) · Config · Output · Logs  │
        └──────────────────────────────────────────────────┘
```

Reglas:

- La CLI **no** conoce detalles HTTP.
- El service **no** parsea JSON de transporte.
- El client SAPI es una librería (`lib.rs`) reutilizable por cualquier UI.
- Todo es inyectable para poder testear con mocks sin red.

## 3. Estructura de módulos

```
src/
  lib.rs               expone la librería (client + modelos + errores)
  main.rs              binario: parsea CLI, init tracing, dispatch
  cli.rs               clap: comandos y flags globales
  config.rs            AppConfig (hosts, profile, device-id) desde dirs/env
  error.rs             Error tipado + mapeo a exit codes
  output.rs            renderizado human/JSON, redacción de secretos
  progress.rs          barra de progreso (indicatif), desacoplada
  api/
    mod.rs
    client.rs          O2CloudClient: transporte, envelope, sesión, retries
    models.rs          modelos serde (Quota, Folder, MediaItem, UploadResult, …)
    media.rs           listar + detalle + estado de validación
    folders.rs         listar carpetas
    quota.rs           quota
    auth.rs            whoami (probe de sesión)
    upload.rs          subida multipart streaming
  auth/
    mod.rs
    session.rs         Session: cookie header, extracción validationKey/JSESSIONID
    store.rs           trait SecretStore + KeyringStore + EnvStore + MemoryStore
    browser.rs         login asistido por navegador (BrowserOpener)
    webview.rs         login con WebView embebida (feature `webview`, Linux)
  service/
    mod.rs             resolución de rutas remotas, identidad
    status.rs quota.rs ls.rs tree.rs stat.rs upload.rs download.rs files.rs
  gui/                 interfaz gráfica (feature `gui`)
    mod.rs             `run()` de eframe
    state.rs           view-model (testeable)
    worker.rs          puente async (tokio + canales)
    app.rs             UI (egui)
  gui_main.rs          binario `o2cloud-gui` (requiere feature `gui`)
```

## 4. Cliente SAPI (`O2CloudClient`)

Responsabilidades separadas dentro del cliente:

| Responsabilidad | Dónde |
| --------------- | ----- |
| Transporte HTTP | `reqwest::Client` (rustls, cookie manual) |
| Autenticación/sesión | `Session` + `auth/store` (adjunta `Cookie` + `?validationkey=`) |
| Parseo de envelopes | `client::parse_envelope` |
| Operaciones | `api/{media,folders,quota,auth}.rs` |

Construcción de cada petición:

1. URL = `<base_url>/sapi` + path.
2. Query params + `validationkey=<vk>` (si hay sesión).
3. Header `Cookie: <cookie header>` (JSESSIONID + validationKey + …).
4. Headers por defecto: `User-Agent`, `Accept: application/json`.
5. Enviar; mapear status HTTP → error tipado.
6. Parsear envelope: `data` vs `error` (un `error` puede venir con HTTP 200).

### Envelope

```rust
struct Envelope<T> { data: Option<T>, error: Option<SapiError> }
```

El parser trabaja sobre `serde_json::Value`:

- si hay `error` no vacío → mapear a `Error::Sapi{code,message}` (con mapeo por
  prefijo `SEC`/`AUT`→Unauthorized, `QUO`→QuotaExceeded, `MED-1000`→NotFound);
- si hay `data` → deserializar `data` a `T`;
- si no hay envelope → devolver el objeto tal cual (endpoints sin envelope).

## 5. Modelo de errores

```rust
pub enum Error {
    Authentication(String),   // no hay sesión local (no logueado)
    Unauthorized(String),     // 401/403 (sesión caducada/inválida)
    NotFound(String),
    Conflict(String),
    QuotaExceeded(String),
    RateLimited(String),
    Transport(String),        // fallo de conexión/timeout
    Protocol(String),         // status 4xx inesperado
    Server(String),           // 5xx
    Sapi { code: String, message: String },  // error SAPI dentro de 200
    InvalidResponse(String),  // JSON no parseable o forma incorrecta
    LocalIo(String),
    Cancelled,
}
```

**Códigos de salida estables** (para `--json`/scripts):

| Código | Significado |
| ------ | ----------- |
| 0 | éxito |
| 1 | uso incorrecto |
| 2 | autenticación / no autorizado |
| 3 | no encontrado |
| 4 | conflicto |
| 5 | red/transporte |
| 6 | rate limited |
| 7 | cuota superada |
| 8 | error de servidor / SAPI |
| 9 | respuesta inválida / protocolo |
| 10 | I/O local |
| 11 | cancelado |

## 6. Sesión, secretos y login asistido

- `Session` conserva la cabecera `Cookie` completa (JSESSIONID + validationKey
  + cookies adicionales) y deriva el `validationKey` (32-hex) y el `JSESSIONID`.
- Almacenamiento vía `trait SecretStore`:
  - `KeyringStore` (producción): keyring del sistema (`o2cloud:<profile>`).
    El crate `keyring` **no trae backend por defecto**; se habilita
    `sync-secret-service` en Linux (requiere `libdbus` + Secret Service,
    p. ej. gnome-keyring/KWallet), `apple-native` en macOS y `windows-native`
    en Windows. Sin backend, keyring usa un *mock* no persistente.
  - `EnvStore` (testing/automatización): lee `O2CLOUD_COOKIE`.
  - `MemoryStore` (tests unitarios).
- **Nunca** se imprimen cookies, tokens, `validationKey` ni URLs firmadas.
  `output.rs` redacta cualquier secreto en logs/errores.

### Login asistido por navegador

`o2cloud login` (sin opciones) delega en `auth/browser.rs`:

1. Abre `https://cloud.o2online.es/login` en el navegador del sistema
   (programa directo, sin shell) a través del trait `BrowserOpener`.
2. Muestra instrucciones para capturar la cabecera `Cookie` de una petición
   `/sapi/*` ("Copy as cURL").
3. Lee la cookie pegada, la normaliza (`Session::from_cookie_header`), la
   guarda en el keyring y devuelve la sesión.

`--import-cookie`/`--import-token` siguen disponibles para automatización (sin
interacción).

### Login con WebView embebida (feature `webview`, Linux)

Con `--features webview`, `o2cloud login` (sin flags) abre el portal en una
`WebView` WebKitGTK dentro de una ventana GTK (`auth/webview.rs`) y captura las
cookies del propio motor (`WebView::cookies_for_url`), sin paste manual. Se usa
el bucle de GTK directamente (la `WebView` es `!Send`), y se ejecuta **antes**
de crear el runtime async (necesita el hilo principal). Requiere WebKitGTK 4.1 +
GTK3; por eso es una feature opt-in y no rompe builds sin esas librerías.

Se prefiere **X11/XWayland** (variable `GDK_BACKEND`, configurable con
`O2CLOUD_WEBVIEW_BACKEND`) y se **desactiva el renderizador DMABUF y el
compositing** de `WebKitGTK` (causa habitual de `Failed to create GBM buffer`
y página en blanco). Si la `WebView` no puede inicializarse o la página
no carga en un tiempo, se devuelve `CaptureOutcome::Unavailable` y la CLI cae
al login con el navegador del sistema (con sugerencias de diagnóstico).

Sin la feature (por defecto), el login usa el navegador del sistema y captura
manual, que no necesita dependencias gráficas.


## 7. Configuración

- `AppConfig`: `base_url` (`https://cloud.o2online.es`), `upload_base_url`
  (`https://upload.cloud.o2online.es`), `user_agent`, `device_id`, `profile`.
- No-secretos en `~/.config/o2cloud/config.toml` (sobrescribible por
  `O2CLOUD_*`). Secretos solo en keyring/env.
- `--profile <name>` namespaca keyring y config.

## 8. Output

- `--json`: stdout = exclusivamente JSON (`{"ok":true,"data":...}` o
  `{"ok":false,"error":{...}}`). Logs/progreso a stderr.
- modo humano: tablas/lista legible; errores a stderr.
- Los errores también se serializan como JSON en modo `--json`.

## 9. Subida (`upload`)

- Endpoint: `POST {upload_base_url}/sapi/upload?action=save` (host dedicado).
- Formulario multipart con dos partes: `data` (JSON
  `{"data":{name,size,creationdate,modificationdate,folder}}`) y `file` (bytes).
- Cabeceras: `X-deviceid`, `x-funambol-file-size`; cookie de sesión y
  `?validationkey=`.
- **Streaming**: el archivo se lee de disco con `ReaderStream` y se envía como
  parte con longitud conocida (nunca se carga entero en memoria). El progreso
  se actualiza al consumir cada chunk.
- Cliente HTTP de transferencia sin timeout total (solo timeout de conexión).
- Tras la respuesta (`{success,id,etag,status,type}`) se sondea
  `POST /sapi/media?action=get-validation-status` con backoff exponencial hasta
  confirmar (`status == "U"`). La consistencia es eventual: no se declara
  "disponible" hasta confirmarla.

## 10. Descarga (`download`)

- Se obtiene la metadata del archivo (`stat`) y su campo `url`: una ruta
  firmada relativa `/sapi/download/file?action=get&k=<sig>&node=<n>`.
- La ruta se convierte en absoluta contra el host principal (**nunca**
  `/sapi/sapi/...`) y se hace `GET` en streaming. El token `k` auto-autentica;
  se aplica la cookie de sesión, pero **no** se añade `validationkey` al query
  (evita romper la firma).
- Escritura en `<destino>.part` → `flush` + `sync_all` → `rename` atómico.
  Si la descarga se interrumpe, se elimina el parcial y no queda un archivo
  aparentemente completo.
- El nombre recibido del servidor se sanea (sin separadores ni `..`) para
  evitar path traversal.
- No sobrescribe un fichero existente salvo `--force`.

## 11. Operaciones de archivos y confirmación

- `mkdir` → `POST /sapi/media/folder?action=save` con `{"data":{"name":…,"parent":…}}`.
- `cp` (server-side) → `POST /sapi/media/file?action=copy` con
  `{"data":{"files":[id],"folder":dest}}`.
- `mv` → `POST /sapi/media/folder?action=add-item` (**PARTIAL**: cuerpo
  observado pero no confirmado).
- `rm` → `media/{file,folder}?action=softdelete` (papelera) o `action=delete`
  (permanente). El tipo de recurso de media se elige por `mediatype`.

**Confirmación (Fase 10).** La capa CLI decide si preguntar (la capa servicio no
hace I/O de UI): `--yes` salta la confirmación; en `--json` o sin TTY no se
pregunta y se exige `--yes`; un rechazo del usuario produce `Error::Cancelled`
(exit 11).

## 12. GUI (feature `gui`)

Interfaz gráfica nativa opcional sobre la misma librería (`service`/`api`/`auth`),
con `eframe`/`egui` (0.36). Es un binario aparte (`o2cloud-gui`) activado con
`--features gui`, para no imponer dependencias gráficas al resto.

- `gui/state.rs`: **view-model** puro (`AppState`, migas de pan, selección,
  entradas) y **testeable** sin UI.
- `gui/worker.rs`: puente entre la UI síncrona y el servicio async. Un hilo con
  su propio runtime de tokio ejecuta los `Command`s y devuelve `Event`s por
  canal; despierta a la UI con `Context::request_repaint`.
- `gui/app.rs`: la UI (`eframe::App::ui`), con paneles y ventana de confirmación
  de borrado.

Vistas: explorador (navegación por id de carpeta, crear/subir/descargar/mover/
borrar —también **carpetas**—), **papelera** (listar + vaciar con confirmación),
cuenta (estado + cuota) y sesión (importar cookie / logout). La subida
y descarga muestran **progreso** (el `service` acepta un `ProgressReporter`
alternativo) y los campos locales usan **diálogos nativos** (`rfd`).

## 13. Tests

- Unitarios: parsing de envelope, extracción de `validationKey`, coerción de
  ids, mapeo de errores, formato `YYYYMMDDThhmmssZ`.
- HTTP con `wiremock`: 200+éxito, 200+error SAPI, 401/403/404/429/500,
  adjuntado de cookie+validationkey, quota, listado, detalle.
- Subida: multipart (cabeceras, cuerpo, fecha), respuesta con/sin envelope,
  validación confirmada y pendiente.
- Live manual (nunca en CI): `O2CLOUD_LIVE_TESTS=1 O2CLOUD_COOKIE=...`.

## 14. Dependencias (punto de partida)

| Crate | Uso |
| ----- | --- |
| `tokio` | runtime async |
| `reqwest` | HTTP (rustls, json, multipart, stream) |
| `serde`/`serde_json` | (de)serialización |
| `clap` | CLI |
| `thiserror` | errores tipados |
| `keyring` | secretos |
| `url` | manipulación de URLs |
| `tracing`/`tracing-subscriber` | logging redacted a stderr |
| `time` | fechas `YYYYMMDDThhmmssZ` |
| `indicatif` | barra de progreso |
| `tokio-util` | `ReaderStream` (streaming desde disco) |
| `futures-util` | adaptación del stream (progreso) |
| `wiremock` (dev) | mock HTTP |
| `tempfile` (dev) | ficheros temporales en tests |
| `wry` + `gtk` (opcional, `webview`) | WebView embebida (Linux) |
| `eframe` + `egui` (opcional, `gui`) | interfaz gráfica nativa |
| `rfd` (opcional, `gui`) | diálogos de archivo nativos (portal XDG) |

Se añadirán `directories` (cuando se persista un archivo de config) y otras
cuando lleguen las fases de download/config.
