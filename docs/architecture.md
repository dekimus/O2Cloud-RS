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
  api/
    mod.rs
    client.rs          O2CloudClient: transporte, envelope, sesión, retries
    models.rs          modelos serde (Quota, Folder, MediaItem, …)
    media.rs           listar + detalle
    folders.rs         listar carpetas
    quota.rs           quota
    auth.rs            whoami (probe de sesión)
  auth/
    mod.rs
    session.rs         Session: cookie header, extracción validationKey/JSESSIONID
    store.rs           trait SecretStore + KeyringStore + EnvStore + MemoryStore
  service/
    mod.rs
    status.rs quota.rs ls.rs stat.rs
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

## 6. Sesión y secretos

- `Session` conserva la cabecera `Cookie` completa (JSESSIONID + validationKey
  + cookies adicionales) y deriva el `validationKey` (32-hex) y el `JSESSIONID`.
- Almacenamiento vía `trait SecretStore`:
  - `KeyringStore` (producción): keyring del sistema (`o2cloud:<profile>`).
  - `EnvStore` (testing/automatización): lee `O2CLOUD_COOKIE`.
  - `MemoryStore` (tests unitarios).
- **Nunca** se imprimen cookies, tokens, `validationKey` ni URLs firmadas.
  `output.rs` redacta cualquier secreto en logs/errores.

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

## 9. Tests

- Unitarios: parsing de envelope, extracción de `validationKey`, coerción de
  ids, mapeo de errores.
- HTTP con `wiremock`: 200+éxito, 200+error SAPI, 401/403/404/429/500,
  adjuntado de cookie+validationkey, quota, listado, detalle.
- Live manual (nunca en CI): `O2CLOUD_LIVE_TESTS=1 O2CLOUD_COOKIE=...`.

## 10. Dependencias (punto de partida)

| Crate | Uso |
| ----- | --- |
| `tokio` | runtime async |
| `reqwest` | HTTP (rustls, json, cookies, stream) |
| `serde`/`serde_json` | (de)serialización |
| `clap` | CLI |
| `thiserror` | errores tipados |
| `keyring` | secretos |
| `url` | manipulación de URLs |
| `tracing`/`tracing-subscriber` | logging redacted a stderr |
| `wiremock` (dev) | mock HTTP |

Se añadirán `indicatif` (progreso), `time`/`chrono` (fechas), multipart,
`directories` (cuando se persista un archivo de config), etc. cuando lleguen
las fases de upload/download/config.
