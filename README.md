# o2cloud

Cliente **no oficial** para Linux de **O2 Cloud** (Telefónica España), escrito
en Rust. Gestiona el almacenamiento en la nube de O2 desde la terminal.

> ⚠️ Interoperabilidad no oficial. O2 Cloud no expone una API pública; este
> cliente habla el SAPI de Funambol OneMediaHub observado en el portal web
> `cloud.o2online.es`. No está respaldado por O2/Telefónica y la API puede
> cambiar sin previo aviso.

## Estado

- `o2cloud login --import-cookie '…'` — importa una sesión del navegador.
- `o2cloud logout` — elimina la sesión.
- `o2cloud status` — estado de la sesión.
- `o2cloud quota` — cuota de almacenamiento.
- `o2cloud ls [RUTA]` — lista carpetas y archivos.
- `o2cloud stat RUTA` — detalle de un archivo (id o ruta).
- `o2cloud upload LOCAL... --to CARPETA` — sube archivos (streaming).
- `o2cloud download REMOTO... --to LOCAL` — descarga archivos (streaming).

No implementado todavía: login asistido por navegador, mkdir/copy/move/rm,
sync, FUSE. Ver `docs/implementation-plan.md`.

## Requisitos

- Rust ≥ 1.85 (`cargo`).
- Un *keyring* del sistema operativo para guardar la sesión (Secret Service en
  Linux, keychain en macOS, …).

## Instalación

```bash
cargo build --release
./target/release/o2cloud --help
```

## Autenticación

El portal usa OIDC con autenticación fuerte (SMS), por lo que no hay login
silencioso con usuario/contraseña. Para esta primera versión se importa la
sesión capturada del navegador:

1. Inicia sesión en `https://cloud.o2online.es`.
2. Abre DevTools → pestaña **Network**.
3. Haz clic en cualquier petición a `/sapi/*`.
4. "Copy as cURL" y copia el valor de `-b '…'` (la cabecera `Cookie`, que
   contiene `JSESSIONID` y `validationKey`).

```bash
o2cloud login --import-cookie 'JSESSIONID=...; validationKey=...'
```

La sesión se guarda en el **keyring del sistema** (nunca en texto plano).
Para testing/automatización puedes usar `O2CLOUD_COOKIE='...'`.

Si una llamada devuelve **401**, la sesión ha caducado: repite el login.

## Uso

```bash
o2cloud status
o2cloud quota
o2cloud ls
o2cloud ls /fotos
o2cloud stat 1195003130
o2cloud stat /fotos/imagen.jpg
o2cloud upload ./informe.pdf --to /documentos
o2cloud upload a.txt b.txt --to /backup
o2cloud download 1195003130 --to ./fotos/
o2cloud download /documentos/informe.pdf --to ./informe.pdf
o2cloud download 1195003130 --to ./informe.pdf --force
```

Opciones globales:

```text
--json        salida JSON exclusiva en stdout
--verbose     más detalle en logs (stderr)
--quiet       silenciar logs
--profile P   perfil de configuración
```

Con `--json`, stdout contiene **exclusivamente** JSON; los logs y el progreso
van a stderr. Los códigos de salida son estables (ver `docs/architecture.md`).

## Seguridad

- TLS siempre verificado (rustls).
- Secretos solo en keyring o `O2CLOUD_*`; nunca en logs ni archivos.
- Nunca se imprimen cookies, `validationKey` ni URLs firmadas.
- Descargas y subidas (futuras) siempre en streaming.

## Documentación

- `docs/protocol.md` — especificación del protocolo SAPI reverse-engineered.
- `docs/architecture.md` — diseño y capas.
- `docs/implementation-plan.md` — estado de las fases.
- `MEMORY.md` — conocimiento técnico persistente.

## Desarrollo

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

Los tests usan un backend mockeado (`wiremock`); no requieren red ni
credenciales. Las pruebas live son manuales y requieren
`O2CLOUD_LIVE_TESTS=1 O2CLOUD_COOKIE=...`.

## Licencia

MIT. Ver `LICENSE`.
