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

- [x] Comandos `login`, `logout`, `status`, `quota`, `ls`, `stat`.
- [x] Flags globales `--json`, `--verbose`, `--quiet`, `--profile`.
- [x] `--json`: stdout solo JSON, logs a stderr, sin confirmación.
- [ ] `tree`, `mkdir`, `upload`, `download`, `cp`, `mv`, `rm` (fases siguientes).

## Fase 7 — Upload

- [ ] multipart streaming a `upload.cloud.o2online.es/sapi/upload?action=save`.
- [ ] `data` + `file` parts, `X-deviceid`, `YYYYMMDDThhmmssZ`.
- [ ] validation polling con backoff.

## Fase 8 — Download

- [ ] metadata → url firmada → streaming → `.part` → rename atómico.

## Fase 9 — Operaciones de archivos

- [ ] `mkdir`, `copy` (server-side), `move`, `softdelete`, `delete` permanente.

## Fase 10 — Seguridad/UX

- [ ] confirmación de operaciones destructivas (`--yes` para automatizar).

## Fase 11 — Tests

- [x] Unitarios y mock HTTP para el slice actual.
- [ ] Ampliar a upload/download/delete/copy/move cuando existan.

## Fase 12 — Documentación

- [x] `README.md`, `docs/*`, `MEMORY.md` actualizados.

## Fase 13 — No implementar todavía

- [ ] FUSE, sync bidireccional, shares, álbumes, labels, búsqueda server-side,
  versiones, GUI, daemon.

## Próximos pasos (siguiente iteración)

1. Browser-assisted login (`wry`/`tao` o system browser) desacoplado.
2. `mkdir` + `upload` (multipart streaming + validation polling).
3. `download` (streaming + `.part` + rename atómico).
4. `copy`/`move`/`rm` server-side con confirmación.
