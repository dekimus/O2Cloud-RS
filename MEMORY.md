# MEMORY.md --- Estado técnico persistente del proyecto O2 Cloud Rust

## Propósito

Este archivo conserva conocimiento técnico descubierto durante la
investigación para que agentes posteriores no repitan ingeniería inversa
ya realizada.

Última investigación: 2026-10-07.

## Servicio

-   Producto: O2 Cloud, Telefónica España.
-   Capacidad actual anunciada: 10 TB para clientes con línea móvil O2;
    los clientes de Fibra + Móvil pasan de 1 TB a 10 TB. La ampliación
    fue anunciada el 18/06/2026.
-   Portal: `https://cloud.o2online.es`
-   API namespace: `https://cloud.o2online.es/sapi/`
-   Upload host: `https://upload.cloud.o2online.es`
-   Backend observado: Funambol OneMediaHub / SAPI.
-   No hay API pública oficial documentada.

Fuente oficial de capacidad:
`https://www.telefonica.com/es/sala-comunicacion/prensa/o2-multiplica-diez-almacenamiento-nube-servicio-o2-cloud-amplia-clientes-tarifa-movil/`

## Evidencia de interoperabilidad

Fuente principal: `https://github.com/dbaratech/o2cloud`

Documentos especialmente relevantes:

-   `docs/api-reference.md`
-   `docs/ARCHITECTURE.md`
-   `docs/apk-analysis.md`
-   `docs/testing.md`

Existe además:

`https://github.com/danielperez9430/o2cloud-cli`

La existencia de dos implementaciones independientes reduce el riesgo de
basarse en una única observación, pero ninguna constituye una API
oficial de O2.

## Autenticación

OIDC/OAuth2 Authorization Code:

-   IdP: `https://t3.o2online.es/acceso/`
-   client_name: `O2CLOUD_WEB`
-   client_id: `7f8afae4-30e7-4591-b4a8-4fc08d545d1d`
-   scope: `openid`
-   acr_values: `2`
-   redirect: `https://cloud.o2online.es/sapi/login/oauth`

El flujo termina creando una sesión SAPI.

Credenciales de sesión observadas:

-   `JSESSIONID`
-   `validationKey`

Ambas son necesarias. `validationKey` se manda como cookie y query
parameter `validationkey`.

El SMS/strong auth impide un login CLI completamente silencioso con
usuario/contraseña.

Estrategia preferida:

1.  browser-assisted login;
2.  capturar sesión;
3.  keyring;
4.  renovar cuando llegue 401.

Fallback útil para desarrollo:

-   DevTools;
-   Network;
-   request `/sapi/*`;
-   Copy as cURL;
-   extraer Cookie con `JSESSIONID` + `validationKey`.

## Envelope

Éxito:

``` json
{"data": {}, "responsetime": 123}
```

Error:

``` json
{
  "error": {
    "code": "...",
    "message": "...",
    "parameters": [],
    "cause": "..."
  },
  "responsetime": 123
}
```

Un HTTP 200 puede contener un error SAPI. Siempre inspeccionar el JSON.

## Endpoints confirmados

### Quota

`GET /sapi/media?action=get-storage-space&softdeleted=true`

### Profile

`GET /sapi/profile?action=get`

### Subscription

`GET /sapi/subscription?action=get`

`GET /sapi/subscription/plan?action=get`

### Folders

`POST /sapi/media/folder?action=get`

`POST /sapi/media/folder?action=save`

`POST /sapi/media/folder?action=delete`

`POST /sapi/media/folder?action=softdelete`

`POST /sapi/media/folder?action=add-item`

`POST /sapi/media/folder?action=remove-item`

`POST /sapi/media/folder?action=zip`

Cuerpos (flujo web, confirmados en el cliente Python):

-   crear/renombrar: `{"data":{"name":<name>[,"parent":<id>]}}`;
-   borrar/papelera: `{"data":{"folders":[<id>]}}`.

La implementación Rust usa `{"data":{name,parentid,magic,creationdate,modificationdate}}`
y `?id=<id>`; seguimos la variante web.

`add-item`/`remove-item`: cuerpo observado pero **PARTIAL**
(`{"data":{"id":<folderid>,"items":[<itemid>]}}`). El movimiento (`mv`) se
implementa como `add-item` y requiere confirmación con tráfico real.

El borrado recursivo de carpetas no está confirmado: con el flujo web, el
listado de media es a nivel de cuenta y no asocia archivos a carpetas.

### Media

`GET /sapi/media?action=get&limit=N`

`POST /sapi/media/file?action=get`

`POST /sapi/media/file?action=delete`

`POST /sapi/media/file?action=softdelete`

`POST /sapi/media/file?action=copy`

`POST /sapi/media?action=get-validation-status`

Cuerpos (confirmados):

-   detalle: `{"data":{"files":[{"id":<id>}]}}`;
-   borrar/papelera: `{"data":{"files":[<id>]}}`;
-   copia server-side: `{"data":{"files":[<id>][,"folder":<dest>]}}`.
-   La variante por tipo (`media/picture|video|audio`) existe; el recurso se
    elige por `mediatype`.

### Download

El detail de media contiene `url`, por ejemplo:

`/sapi/download/file?action=get&k=<signature>&node=<n>`

Ese `url` debe convertirse en absoluto contra
`https://cloud.o2online.es`.

No concatenar dos veces `/sapi`.

El token `k` auto-autentica la descarga. La implementación Python
(confirmada live) aplica la cookie de sesión y hace GET **sin** añadir
`validationkey` al query (añadirlo podría invalidar la firma). Nuestra
implementación sigue ese comportamiento.

`action=export` no es la descarga normal.

### Upload

`POST https://upload.cloud.o2online.es/sapi/upload?action=save`

Multipart con dos partes (CONFIRMED live 2026-07-22):

-   `data`: JSON `{"data":{name,size,modificationdate,creationdate[,folder]}}`
-   `file`: bytes (content-type `application/octet-stream`)

Header:

`X-deviceid: <deviceid>` y `x-funambol-file-size: <bytes>`.
La cookie de sesión (`JSESSIONID`) aplica también al host de subida y
`validationkey` va como query param.

Fechas:

`YYYYMMDDThhmmssZ` (UTC compacto).

Epoch milliseconds provocan `COM-1008`.

Respuesta incluye (puede venir con o sin envoltorio `data`):

-   success
-   id
-   etag
-   status
-   type

Hay consistencia eventual: tras subir, pollear

`POST /sapi/media?action=get-validation-status`

con body `{"data":{"ids":[{"id":<id>}]}}` → `data.ids[]{id,status}`. El estado
`U` indica subido/válido. No declarar "disponible" hasta confirmarlo.

### Revisions

`/sapi/media/revision?action=get`

`/sapi/media/revision?action=restore`

### Thumbnail

`GET /sapi/download/thumbnail?action=get&fdoid=<id>`

## Modelo de media

Listado sparse:

``` text
id
date
mediatype
status
userid
```

Detail:

``` text
id
url
date
creationdate
modificationdate
uploaded
size
name
mediatype
status
etag
softdeleted
```

## Errores ya conocidos

-   `401`: sesión inválida/expirada si faltan credenciales correctas.
-   `MED-1000`: no usar `/sapi/media?action=get` con `data.ids` como
    sustituto de `media/file?action=get`.
-   `COM-1008`: fechas de upload en formato incorrecto; usar
    `YYYYMMDDThhmmssZ`.

## Áreas incompletas

No asumir que están terminadas:

-   búsqueda server-side;
-   shares;
-   albums/labels;
-   restore/empty exacto de papelera;
-   algunos parámetros de export/zip;
-   OAuth bearer/silent refresh del móvil.

La búsqueda puede ser local sobre los listados mientras no exista
evidencia suficiente de un endpoint.

## Discrepancias entre implementaciones (2026-10-07)

Comparando `dbaratech/o2cloud` (Python, flujo web) y
`danielperez9430/o2cloud-cli` (Rust, app de escritorio) se detectaron
diferencias relevantes. Conclusión: **son dos clientes oficiales distintos**
(portal web vs app de escritorio) y existen dos esquemas de autenticación.

### Nombre del backend

-   Python dice "Funambol OneMediaHub / SAPI".
-   Rust dice "Synchronoss OneMediaHub v31".

No es una discrepancia real: **Synchronoss adquirió Funambol** y comercializa
OneMediaHub. Ambos nombres designan el mismo producto.

### Esquemas de autenticación (DOS flujos distintos)

1.  **Web OIDC** (nuestro objetivo, según `AGENTS.md`):
    `JSESSIONID` (cookie httpOnly) + `validationKey` (cookie + query), **sin**
    cabecera `Authorization`.
2.  **Mobile Connect (app de escritorio)** — observado en el cliente Rust:
    -   `POST /sapi/login/mobileconnect?action=start` (form `platform`,
        `msisdn`) → `authorizationurl`;
    -   WebView intercepta `code`+`state` en `clientoauth.html`;
    -   `POST /sapi/credential/mobileconnect?action=validate`
        `{"data":{"code","state"}}` → `access_token`, `expires_in`, `msisdn`,
        `platform`, `lastrefreshdate`;
    -   `POST /sapi/login?action=login&responsetime=true`
        (header `Authorization: oauth <base64>`) → `access_token`,
        `jsessionid`, `validationkey`, `encryption-token`, `roles`;
    -   Las llamadas envían `Authorization: oauth <base64>` **y** la cookie
        `JSESSIONID`; permite *silent re-login* reutilizando `access_token`.

Decisión: implementamos el **flujo web** (1). El flujo 2 queda documentado
como alternativa pero no se implementa.

### Endpoints que difieren

| Área | Web (Python) | Escritorio (Rust) |
| ---- | ------------ | ----------------- |
| Listar media | `GET /media?action=get&limit=N` (sparse) + detalle aparte | `POST /media?action=get` con `{data:{fields:[…]}}` + `limit`/`offset` |
| Subida | multipart `data`+`file` a `/sapi/upload?action=save` | 2 pasos: `/sapi/upload/file?action=save-metadata` + `action=save` (octet-stream) |
| Borrar media | `POST /media/file?action=delete` body `{data:{files:[id]}}` | `POST /media?action=softdelete&id=<id>` |
| Borrar carpeta | body `{data:{folders:[id]}}` | query `?id=<id>` |
| mkdir body | `{name, parent}` | `{name, parentid, magic, creationdate, modificationdate}` |
| Probe de sesión | `profile?action=get` | `profile/role?action=get` |

Seguimos la variante **web** en todos los casos. El detalle de carpeta incluye
`parentid` (la impl. Rust construye el árbol con él).

### Nota sobre listado por carpeta

El listado sparse (`GET /media?action=get`) es a nivel de cuenta. Pero el
**detalle** (`POST /media/file?action=get`) incluye `folder` (carpeta
contenedora; None/ausente = raíz), así que la asociación archivo→carpeta **sí**
está disponible filtrando el detalle (confirmado live).

## Validación live (2026-10-07)

Contrastado contra una cuenta real (sesión web). Correcciones importantes
respecto a lo deducido de las implementaciones de referencia:

-   **mkdir**: cuerpo `{"data":{"name"[, "parentid"]}}` (el campo `parent` que
    usa el cliente Python es **ignorado**); respuesta real
    `{"data":{"folder":{"id":…},"success":"…"}}`.
-   **detalle media** `POST /media/file?action=get`: devuelve el detalle de
    **TODOS** los archivos (ignora el filtro de `id`); filtrar en cliente.
    `creationdate`/`modificationdate`/`uploaded` son **epoch-ms enteros**
    (no cadena/bool); incluye `folder`; sin `contenttype`.
-   **listado de carpetas**: trae `parentid` (a veces omitido en primer nivel).
-   **move** (`mv`): `POST /media/folder?action=add-item` con
    `{"data":{"folderid":<dest>,"items":[<item>]}}` (con `id` da `COM-1011`).
    Mueve el elemento; el detalle pasa a exponer `folder`.
-   **borrado**:
    -   papelera media: `POST /media?action=softdelete&id=<id>`
        ("Item successfully moved to trash");
    -   permanente media: `POST /media/file?action=delete` body
        `{"data":{"files":[id]}}` ("Files removed successfully");
    -   papelera carpeta: `POST /media/folder?action=softdelete&id=<id>`
        ("Folders have been softdeleted");
    -   permanente carpeta: `POST /media/folder?action=delete` body
        `{"data":{"folders":[id]}}`.
    El `media/file?action=softdelete` **con body es un no-op**.
-   **subida**: `success` es una **cadena** ("Media uploaded successfully").
-   **papelera**: listar con `GET /sapi/media/trash?action=get` →
    `data.entries[]{date,id,name,type}` (`type`: `media`|`folder`). El vaciado
    se hace borrando **permanentemente** cada entrada (no hay endpoint masivo
    confirmado); restaurar sigue sin confirmar.
-   **cp/copy**: **UNCONFIRMED** (`COM-1011` con todos los cuerpos probados).

## Decisiones de arquitectura

Rust:

``` text
CLI
 ↓
Service
 ↓
SAPI client
 ↓
HTTP/auth/keyring
```

El cliente SAPI debe ser reutilizable por CLI/TUI/FUSE/rclone.

Archivos grandes:

-   streaming obligatorio;
-   no cargar 10 TB ni un archivo grande entero en memoria.

Secretos:

-   keyring;
-   nunca logs;
-   nunca repositorio;
-   nunca configuración normal.

IMPORTANTE (keyring 3.x): el crate `keyring` **no incluye ningún backend por
defecto**; sin una feature de backend usa un almacén *mock* no persistente
(las sesiones "se guardan" pero no se leen en otro proceso). Hay que habilitar
`sync-secret-service` (Linux, requiere `libdbus` + Secret Service),
`apple-native` (macOS) o `windows-native` (Windows).

Login asistido por navegador (2026-10-07): `o2cloud login` sin opciones abre
el portal en el navegador del sistema y captura la cookie pegada por el
usuario. La capa está desacoplada tras `BrowserOpener`.

Login con WebView embebida (feature opt-in `webview`, Linux): usa `wry` +
`gtk` (WebKitGTK 4.1) para abrir el portal y leer las cookies del motor
(`WebView::cookies_for_url`), capturando `JSESSIONID` + `validationKey` sin
intervención del usuario. La `WebView` es `!Send` y se ejecuta en el hilo
principal (bucle GTK) antes del runtime async. No es una feature por defecto
para no imponer WebKitGTK a todas las builds.

IMPORTANTE (WebView en Linux, 2026-10-07): WebKitGTK/Wayland puede fallar con
`Gdk-Message: Error 71 dispatching to Wayland display` y arrastrar a WebKit
(`internallyFailedLoadTimerFired`). Por eso se fuerza `GDK_BACKEND=x11` por
defecto (override con `O2CLOUD_WEBVIEW_BACKEND`/`GDK_BACKEND`). Además, el
renderizador DMABUF/compositing de WebKitGTK provoca `Failed to create GBM
buffer` y página en blanco; se desactiva por defecto
(`WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`), sin
impedir que el usuario lo sobrescriba. Si la página no carga, se cae al login
con navegador del sistema; en contenedores puede hacer falta
`WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` y/o `LIBGL_ALWAYS_SOFTWARE=1`.

Layout (2026-10-07): con `wry` + GTK la `WebView` **no se redimensiona sola**
(queda en su tamaño por defecto). Hay que fijar sus bounds al tamaño de la
ventana con `WebView::set_bounds` en `connect_size_allocate`; el builder de
`build_gtk(&gtk::Fixed)` no lo hace automáticamente.

## GUI (2026-10-07)

GUI nativa opcional con `eframe`/`egui` **0.36** (feature `gui`, binario
`o2cloud-gui`). Nota de API: en 0.36 el trait es `eframe::App::ui(&mut self,
&mut Ui, &mut Frame)` (no `update`) y los paneles son `egui::Panel::{top,bottom,
left,right}` mostrados dentro de un `Ui` (ya no `TopBottomPanel`/`SidePanel`).

Arquitectura: view-model puro y testeable (`gui/state.rs`) + puente async
(`gui/worker.rs`: hilo con runtime tokio, `Command`/`Event` por canal y
`Context::request_repaint`) + UI (`gui/app.rs`). Reutiliza la misma librería que
la CLI. La subida/descarga muestran progreso: `ProgressReporter` es un sink
opcional (`ProgressSink`) con implementaciones `indicatif` (CLI) y callback
(GUI); las opciones `UploadOptions`/`DownloadOptions` aceptan `reporter`. Los
diálogos de archivo usan `rfd` (portal XDG, sin GTK). Tras una mutación el
worker emite `Event::Changed` y la UI recarga la vista (carpeta, cuota y
papelera) automáticamente; el login reemite la carga inicial y el logout limpia
el estado.

Login desde la GUI: botón de **WebView embebida** (si la feature `webview` está
activa; `capture_cookie_header` se ejecuta en un **hilo secundario**, se ha
verificado que GTK funciona ahí), botón de **abrir navegador** del sistema y
campo para pegar la cookie.

## Seguridad (revisión 2026-10-07)

Revisión completa en `docs/security-review.md` y política en `SECURITY.md`. Se
corrigieron 7 puntos; los más relevantes:

-   **SSRF/fuga de cookie (ALTO)**: la descarga de `url` firmada se restringe al
    mismo host/esquema (antes una `url` absoluta ajena habría recibido la cookie).
-   **Firmas `k=…` no redactadas (MEDIO)**: `redact_secrets` cubre ahora `k`,
    `signature`, `token`, etc., con límite de clave (no enmascara `link=`).
-   **`Debug` derivado en `Session`/`MediaItem`/`MemoryStore` (MEDIO/BAJO)**:
    implementaciones manuales que redactan cookie/validationKey/url.
-   **Inyección de argumentos del navegador (BAJO)**: se valida `http(s)://`.
-   **Cookie visible en la GUI (BAJO)**: campo enmascarado.

Verificado correcto: TLS siempre verificado (rustls), sin `unsafe`, sin shell ni
`eval`, secretos solo en keyring/env, redacción en errores/logs, `url` con
`skip_serializing`, saneado anti-traversal y `.part`+rename atómico.

Riesgo residual: `login --import-cookie` expone el secreto en `ps`/historial
(usar WebView/navegador). Recomendado ejecutar `cargo audit` en CI.

## Fuentes externas

1.  Telefónica, anuncio oficial de 10 TB:
    `https://www.telefonica.com/es/sala-comunicacion/prensa/o2-multiplica-diez-almacenamiento-nube-servicio-o2-cloud-amplia-clientes-tarifa-movil/`

2.  Cliente Python y API reverse-engineered:
    `https://github.com/dbaratech/o2cloud`

3.  API observada:
    `https://github.com/dbaratech/o2cloud/blob/main/docs/api-reference.md`

4.  Arquitectura:
    `https://github.com/dbaratech/o2cloud/blob/main/docs/ARCHITECTURE.md`

5.  Cliente Rust existente:
    `https://github.com/danielperez9430/o2cloud-cli`

6.  Condiciones particulares O2 Cloud:
    `https://web.static.o2online.es/media/public/condiciones_particulares_o2_cloud.pdf`

## Nota legal/técnica

El proyecto debe presentarse como cliente de interoperabilidad no
oficial. No afirmar que está respaldado por O2/Telefónica.

El agente debe respetar las condiciones del servicio y la legislación
aplicable.

No realizar pruebas destructivas contra cuentas reales sin
consentimiento explícito del propietario.

## Regla de mantenimiento de MEMORY.md

Cuando se descubra un endpoint nuevo:

1.  añadirlo aquí;
2.  indicar si está `CONFIRMED`, `OBSERVED` o `UNCONFIRMED`;
3.  registrar método, path, body y respuesta relevante;
4.  añadir test;
5.  indicar la fuente de evidencia.

No sustituir una observación confirmada por una suposición más reciente
sin explicar el cambio.
