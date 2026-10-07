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

## Fase 4 — Autenticación (importación) ✅

- [x] `SecretStore` con keyring + env + memory.
- [x] `login --import-cookie` (y `--import-token`).
- [x] Detección de 401 → sesión caducada.
- [ ] Browser-assisted login (siguiente iteración).

## Fase 5 — Primer vertical slice ✅

- [x] `status` (whoami via `profile?action=get`).
- [x] `quota` (via `media?action=get-storage-space`).
- [x] `ls [REMOTE]` (carpetas + media + detalle para nombres).
- [x] `stat REMOTE` (detalle de un archivo).

## Fase 6 — CLI ✅ (parcial)

- [x] Comandos `login`, `logout`, `status`, `quota`, `ls`, `stat`, `upload`,
  `download`.
- [x] Flags globales `--json`, `--verbose`, `--quiet`, `--profile`.
- [x] `--json`: stdout solo JSON, logs a stderr, sin confirmación.
- [ ] `tree`, `mkdir`, `cp`, `mv`, `rm` (fases siguientes).

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

## Fase 9 — Operaciones de archivos

- [ ] `mkdir`, `copy` (server-side), `move`, `softdelete`, `delete` permanente.

## Fase 10 — Seguridad/UX

- [ ] confirmación de operaciones destructivas (`--yes` para automatizar).

## Fase 11 — Tests

- [x] Unitarios y mock HTTP para lectura, subida y descarga.
- [ ] Ampliar a delete/copy/move cuando existan.

## Fase 12 — Documentación

- [x] `README.md`, `docs/*`, `MEMORY.md` actualizados.

## Fase 13 — No implementar todavía

- [ ] FUSE, sync bidireccional, shares, álbumes, labels, búsqueda server-side,
  versiones, GUI, daemon.

## Próximos pasos (siguiente iteración)

1. `mkdir` + `copy`/`move`/`rm` server-side con confirmación.
2. Browser-assisted login (`wry`/`tao` o system browser) desacoplado.
3. `tree` en la CLI.
