# o2cloud

Cliente **no oficial** para Linux de **O2 Cloud** (Telefónica España), escrito
en Rust. Gestiona el almacenamiento en la nube de O2 desde la terminal y con
una **interfaz gráfica nativa**.

> ⚠️ Interoperabilidad no oficial. O2 Cloud no expone una API pública; este
> cliente habla el SAPI de Funambol OneMediaHub observado en el portal web
> `cloud.o2online.es`. No está respaldado por O2/Telefónica y la API puede
> cambiar sin previo aviso.

## Características

**CLI** (`o2cloud`)

- `login` (WebView embebida, navegador del sistema o importar cookie), `logout`
- `status`, `quota`
- `ls [RUTA]`, `tree [RUTA]`, `stat REMOTO`
- `mkdir REMOTO`
- `upload LOCAL... --to CARPETA` (streaming)
- `download REMOTO... --to LOCAL` (streaming, `.part` + rename atómico)
- `mv ORIGEN DESTINO` (server-side)
- `rm REMOTO [--permanent] [-y]` (papelera por defecto, con confirmación)
- opciones globales `--json`, `--verbose`, `--quiet`, `--profile`
- `cp ORIGEN DESTINO` — *no disponible todavía* (protocolo sin confirmar)

**GUI nativa** (`--features gui`)

- explorador: crear/subir/descargar/mover/borrar archivos **y carpetas**
- **papelera**: listar y vaciar (con confirmación)
- cuenta (estado + cuota) y sesión (login/logout)
- diálogos de archivo nativos, **progreso** de transferencias y
  **auto-refresco** tras cada acción

Pendiente: `cp` server-side, restaurar de papelera, `sync`, FUSE.
Ver `docs/implementation-plan.md`.

## Requisitos

- Rust ≥ 1.85 (`cargo`).
- Un *keyring* del sistema operativo para guardar la sesión (Secret Service en
  Linux, keychain en macOS, …).

## Instalación

```bash
git clone https://github.com/dekimus/O2Cloud-RS
cd O2Cloud-RS
cargo build --release
./target/release/o2cloud --help
```

## Autenticación

El portal usa OIDC con autenticación fuerte (SMS), por lo que no hay login
silencioso con usuario/contraseña. El flujo recomendado es **asistido por
navegador**:

```bash
o2cloud login
```

Esto abre `https://cloud.o2online.es/login` en tu navegador. Inicia sesión
(teléfono + SMS); después la CLI te guía para capturar la cookie de sesión:

1. Abre DevTools (F12) → pestaña **Network**.
2. Recarga y haz clic en cualquier petición a `/sapi/*`.
3. "Copy as cURL" y copia el valor de `-b '…'` (la cabecera `Cookie`, que
   contiene `JSESSIONID` y `validationKey`).
4. Pégala en la CLI.

Para automatización (sin interacción) o si prefieres hacerlo a mano:

```bash
o2cloud login --import-cookie 'JSESSIONID=...; validationKey=...'
# o una sesión por variable de entorno (testing):
export O2CLOUD_COOKIE='JSESSIONID=...; validationKey=...'
```

La sesión se guarda en el **keyring del sistema** (nunca en texto plano).

> **Requisito del keyring (Linux):** la CLI necesita un backend de Secret
> Service (`libdbus` + gnome-keyring/KWallet). Sin él, no se puede persistir la
> sesión de forma segura.

Si una llamada devuelve **401**, la sesión ha caducado: repite `o2cloud login`.

### Login con WebView embebida (opcional)

Con la feature `webview` (solo Linux), `o2cloud login` abre el portal en una
`WebView` embebida y captura la sesión automáticamente (sin copiar/pegar):

```bash
cargo build --release --features webview
./target/release/o2cloud login
```

Requiere WebKitGTK 4.1 + GTK3:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
```

Sin la feature, `login` usa el navegador del sistema y la captura manual, que no
necesita dependencias gráficas.

La WebView se apoya en **X11/XWayland por defecto** y **desactiva el
renderizador DMABUF y el compositing de WebKitGTK** (es la causa habitual de
`Failed to create GBM buffer` y de una ventana en blanco). Si aun así falla, lo
detecta y **cae automáticamente** al login con el navegador del sistema.
Variables útiles (todas sobrescriben los valores por defecto):

```bash
GDK_BACKEND=x11                                   # forzar X11 (por defecto)
GDK_BACKEND=wayland                               # forzar Wayland
LIBGL_ALWAYS_SOFTWARE=1                           # renderizado software
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 # contenedores (menos seguro)
O2CLOUD_WEBVIEW_BACKEND=wayland                   # alternativa a GDK_BACKEND
```

## Uso

```bash
o2cloud status
o2cloud quota
o2cloud ls
o2cloud ls /fotos
o2cloud tree
o2cloud tree /fotos
o2cloud stat 1195003130
o2cloud stat /fotos/imagen.jpg
o2cloud upload ./informe.pdf --to /documentos
o2cloud upload a.txt b.txt --to /backup
o2cloud download 1195003130 --to ./fotos/
o2cloud download /documentos/informe.pdf --to ./informe.pdf
o2cloud download 1195003130 --to ./informe.pdf --force

o2cloud mkdir /documentos/2026
o2cloud cp /documentos/informe.pdf /backup
o2cloud mv 1195003130 /archivo
o2cloud rm /documentos/viejo.pdf          # a la papelera (pide confirmación)
o2cloud rm 1195003130 --permanent -y      # borrado permanente, sin preguntar
```

### Operaciones destructivas

`rm` pide confirmación interactiva (`¿Eliminar …? [y/N]`). Para scripts usa
`--yes`/`-y`. En modo `--json` o sin TTY **no** se pregunta: es obligatorio
`--yes`, y si falta se aborta con un error.

Opciones globales:

```text
--json        salida JSON exclusiva en stdout
--verbose     más detalle en logs (stderr)
--quiet       silenciar logs
--profile P   perfil de configuración
```

Con `--json`, stdout contiene **exclusivamente** JSON; los logs y el progreso
van a stderr. Los códigos de salida son estables (ver `docs/architecture.md`).

## Interfaz gráfica (opcional)

Incluye una GUI nativa (`eframe`/`egui`) sobre la misma librería:

```bash
cargo build --release --features gui
./target/release/o2cloud-gui
```

Permite ver estado/cuota, navegar por carpetas y crear/subir/descargar/mover/
borrar archivos **y carpetas**, con **diálogos nativos** para elegir rutas
locales, barra de **progreso** en subida/descarga, pestaña de **Papelera**
(listar y vaciar con confirmación) e **iniciar/cerrar sesión**. La vista se
**recarga automáticamente** tras cada acción. Es una feature opt-in para no
imponer dependencias gráficas al resto.

El **login** desde la GUI ofrece:
- **WebView embebida** (`--features gui,webview`, Linux): captura la sesión
  automáticamente.
- **Abrir navegador**: inicia sesión en tu navegador y pega la cookie.
- **Pegar cookie** manualmente.

```bash
# con WebView embebida
cargo run --release --features gui,webview --bin o2cloud-gui
```

## Seguridad

- TLS siempre verificado (rustls); sin `unsafe`, sin shell ni `eval`.
- Secretos solo en el keyring o `O2CLOUD_*`; nunca en logs ni archivos.
- Nunca se imprimen cookies, `validationKey` ni URLs firmadas (se redactan).
- Subidas y descargas en **streaming** (sin cargar el archivo en memoria).
- Descarga restringida al mismo host/esquema (anti-SSRF) y saneado de nombres.

Detalle completo en `SECURITY.md` y `docs/security-review.md`.

## Documentación

- `docs/protocol.md` — especificación del protocolo SAPI reverse-engineered.
- `docs/architecture.md` — diseño y capas.
- `docs/implementation-plan.md` — estado de las fases.
- `docs/security-review.md` — auditoría de seguridad.
- `SECURITY.md` — modelo de seguridad y cómo reportar.
- `MEMORY.md` — conocimiento técnico persistente.

## Desarrollo

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

Los tests usan un backend mockeado (`wiremock`); no requieren red ni
credenciales. Las pruebas live son manuales, están ignoradas por defecto y
requieren una sesión real:

```bash
O2CLOUD_LIVE_TESTS=1 O2CLOUD_COOKIE='JSESSIONID=…; validationKey=…' \
  cargo test --test live -- --ignored --nocapture
```

## Licencia

MIT. Ver `LICENSE`.
