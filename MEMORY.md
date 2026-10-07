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

El listado web de media es **a nivel de cuenta** (sin asociación fiable
archivo→carpeta). El parámetro `?folder=<id>` se reenvía pero su
comportamiento de filtrado no está confirmado al 100%.

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
