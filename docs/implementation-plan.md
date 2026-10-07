# Plan de implementación

Estado de cada fase. El check (✅) marca lo completado en esta primera
iteración.

## Fase 0 — Reconocimiento ✅

- [x] Leer `AGENTS.md` y `MEMORY.md`.
- [x] Estudiar `dbaratech/o2cloud` (docs + código).
- [x] Estudiar `danielperez9430/o2cloud-cli` (código).
- [x] Crear `docs/protocol.md` (tabla endpoint/método/body/respuesta/evidencia/estado).
- [x] Documentar discrepancias entre implementaciones.

## Fase 1 — Arquitectura ✅

- [x] Definir capas CLI → service → api → infra.
- [x] Crear `docs/architecture.md`.

## Fase 2 — Proyecto Rust ✅

- [x] `Cargo.toml` con dependencias mínimas y actuales.
- [x] `lib.rs` + `main.rs` (librería reutilizable + binario).
- [x] warnings estrictos (`#![warn(...)]` / `deny` donde proceda).
- [x] `cargo fmt --check`, `cargo check`, `cargo test`, `cargo clippy` limpios.

## Fase 3 — Cliente HTTP/SAPI ✅

- [x] `O2CloudClient` con transporte/auth/envelope/operaciones separados.
- [x] Parser de envelopes (HTTP error / SAPI error / éxito / respuesta inválida).
- [x] Errores tipados + mapeo de códigos.
- [x] Tests con mock HTTP.

## Fase 4 — Autenticación ✅

- [x] `SecretStore` con keyring del sistema + env + memory.
- [x] `login --import-cookie` (y `--import-token`).
- [x] detección de 401 → sesión caducada.
- [x] **login asistido por navegador**: `o2cloud login` (sin flags) abre el
  portal en el navegador del sistema, guía la captura de la cookie
  (`JSESSIONID` + `validationKey`) y la guarda en el keyring. Capa
  `BrowserOpener` desacoplada.
- [x] **login con WebView embebida** (feature opcional `webview`, Linux):
  abre el portal en una `WebView` WebKitGTK, captura las cookies del motor y
  las guarda en el keyring. Verificado end-to-end con un backend mock.
- [x] backend de keyring por plataforma (`sync-secret-service` en Linux,
  `apple-native`/`windows-native`); el crate `keyring` no trae ninguno por
  defecto.

## Fase 5 — Primer vertical slice ✅

- [x] `status` (whoami via `profile?action=get`).
- [x] `quota` (via `media?action=get-storage-space`).
- [x] `ls [REMOTE]` (carpetas + media + detalle para nombres).
- [x] `stat REMOTE` (detalle de un archivo).

## Fase 6 — CLI ✅

- [x] Comandos `login`, `logout`, `status`, `quota`, `ls`, `tree`, `stat`,
  `upload`, `download`, `mkdir`, `cp`, `mv`, `rm`.
- [x] Flags globales `--json`, `--verbose`, `--quiet`, `--profile`.
- [x] `--json`: stdout solo JSON, logs a stderr, sin confirmación.
- [x] `tree` (jerarquía de carpetas vía `parentid`, con fallback plano).

## Fase 7 — Upload ✅

- [x] multipart streaming a `upload.cloud.o2online.es/sapi/upload?action=save`.
- [x] partes `data` (JSON `{"data":{name,size,creationdate,modificationdate,folder}}`)
  y `file`, cabecera `X-deviceid`, fechas `YYYYMMDDThhmmssZ`.
- [x] barra de progreso (stderr, solo con TTY y sin `--json`/`--quiet`).
- [x] validation polling (`get-validation-status`) con backoff exponencial.
- [x] archivo no se carga en memoria (streaming desde disco).
- [x] tests: multipart (cabeceras, cuerpo, fecha), validación confirmada y
  pendiente, respuesta sin envelope.

## Fase 8 — Download ✅

- [x] metadata → URL firmada → streaming.
- [x] escritura en `<destino>.part` + `rename` atómico al finalizar.
- [x] `flush` + `sync_all` antes del rename; limpieza del parcial si falla.
- [x] limpieza de nombre recibido del servidor (anti path traversal).
- [x] no sobrescribe por defecto (`--force` para hacerlo).
- [x] tests: streaming + rename, sin parcial, conflict sin `--force`, overwrite
  con `--force`, error 500 sin fichero.

## Fase 9 — Operaciones de archivos ✅ (validado live 2026-10-07)

- [x] `mkdir` → `POST /sapi/media/folder?action=save` con `parentid` (el campo
  `parent` del cliente Python es ignorado); respuesta `{folder:{id},success}`.
- [x] `mv` → `POST /sapi/media/folder?action=add-item` con
  `{"data":{"folderid":…,"items":[id]}}` (confirmado live).
- [x] `rm` (papelera) → `media?action=softdelete&id=` /
  `media/folder?action=softdelete&id=` (query).
- [x] `rm --permanent` → `media/file?action=delete` body `files:[id]` /
  `media/folder?action=delete` body `folders:[id]`.
- [ ] `cp` (server-side) → **UNCONFIRMED** (`COM-1011`); devuelve error claro.
- [x] tests de forma de las peticiones (save/add-item/delete/softdelete).

## Fase 10 — Seguridad y UX ✅

- [x] confirmación interactiva `¿Eliminar …? [y/N]` para `rm`.
- [x] `--yes` (`-y`) para automatización.
- [x] en `--json` o sin TTY no se pregunta: se exige `--yes` (si no, error).
- [x] rechazo del usuario → `Error::Cancelled` (exit 11).
- [x] verificado end-to-end con backend mock (y/n).

## Fase 11 — Tests

- [x] Unitarios y mock HTTP para lectura, subida, descarga y mutaciones.
- [x] Casos límite: HTTP 400 (SAPI y no-JSON), cuota SAPI (`QUO-*`),
  seguridad SAPI (`SEC-*`), sesión expirada, archivo vacío, archivos grandes
  (streaming de 3 MiB en subida y descarga) y fallo de red en subida.
- [x] Tests live **ignorados** y con guarda `O2CLOUD_LIVE_TESTS`/`O2CLOUD_COOKIE`
  (solo lectura; nunca por defecto).

## Fase 12 — Documentación

- [x] `README.md`, `docs/*`, `MEMORY.md` actualizados.

## Fase 13 — No implementar todavía

- [ ] FUSE, sync bidireccional, shares, álbumes, labels, búsqueda server-side,
  versiones, daemon.

## Fase 14 — GUI ✅

- [x] GUI nativa opcional (`eframe`/`egui` 0.36, feature `gui`, binario
  `o2cloud-gui`).
- [x] view-model testeable (`gui/state.rs`) + puente async (`gui/worker.rs`).
- [x] vistas: explorador (navegar/crear/subir/descargar/mover/borrar), cuenta
  (estado + cuota), sesión (importar cookie / logout).
- [x] progreso de subida/descarga (`ProgressReporter` con callback) y diálogos
  de archivo nativos (`rfd`).
- [x] borrado de carpetas (selección de carpeta) y pestaña **Papelera**
  (listar + vaciar con confirmación).
- [x] **auto-refresco** tras una acción (`Event::Changed` → recarga carpeta,
  cuota y papelera; limpia la selección si el elemento desaparece).
- [x] verificado que arranca (smoke test bajo Xvfb).

## Próximos pasos (siguiente iteración)

1. Confirmar el cuerpo de `cp` (copy) y el borrado recursivo de carpetas.
2. Extender la `WebView` a macOS/Windows (en Linux ya está tras la feature `webview`).
3. Paginación de listados si se confirma el parámetro de continuación del flujo web.
