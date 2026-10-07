# Protocolo SAPI de O2 Cloud — especificación

> Especificación reverse-engineered del protocolo interno de O2 Cloud
> (Telefónica España). No existe API pública. Este documento consolida la
> evidencia de dos implementaciones independientes y la documentación
> observada. Cada operación está clasificada según su nivel de evidencia.

**Última actualización:** 2026-10-07

## 1. Fuentes de evidencia

| Fuente | Tipo | Papel |
| ------ | ---- | ----- |
| `dbaratech/o2cloud` (Python) | Código + docs + tráfico live | **Principal** (flujo web) |
| `danielperez9430/o2cloud-cli` (Rust) | Código | Corroboración (flujo app de escritorio) |
| `docs/api-reference.md`, `docs/ARCHITECTURE.md`, `docs/apk-analysis.md` (Python) | Documentación | Referencia del flujo web |
| Tráfico real del portal | Observación | Confirmación live 2026-07-22 |

## 2. Plataforma backend

- Host principal: `https://cloud.o2online.es`
- Namespace API: `https://cloud.o2online.es/sapi/`
- Host de subida: `https://upload.cloud.o2online.es`
- Backend: **Funambol OneMediaHub** (SAPI). La implementación Rust lo
  denomina **"Synchronoss OneMediaHub v31"**. No es una discrepancia real:
  **Synchronoss adquirió Funambol** y comercializa OneMediaHub; ambos
  nombres designan el mismo producto.

## 3. Autenticación

Existen **dos esquemas de autenticación** distintos, correspondientes a dos
clientes oficiales diferentes:

### 3.1 Flujo web OIDC (nuestro objetivo) — CONFIRMED

Este es el flujo que usa el portal web y el que sigue este cliente.

1. El usuario accede a `cloud.o2online.es/login`.
2. Redirección al IdP de Mi O2 (`https://t3.o2online.es/acceso/`) con:
   - `client_name=O2CLOUD_WEB`
   - `client_id=7f8afae4-30e7-4591-b4a8-4fc08d545d1d`
   - `scope=openid`
   - `acr_values=2` (fuerza autenticación fuerte / SMS)
   - `redirect_uri=https://cloud.o2online.es/sapi/login/oauth`
   - `response_type=code`, `state`, `nonce`, `prompt=login consent`,
     `display=page`, `vip_conf_mode=app`
3. El IdP redirige con `?code=...&state=...` al `redirect_uri`.
4. El SAPI intercambia el código y establece una sesión.

**Credenciales de sesión resultantes (ambas necesarias):**

| Credencial | Tipo | Dónde viaja |
| ---------- | ---- | ----------- |
| `JSESSIONID` | cookie servlet httpOnly | cookie `Cookie: JSESSIONID=...` |
| `validationKey` | token 32-hex | cookie `validationKey=...` **y** query `?validationkey=...` |

- `validationKey` por sí solo devuelve **HTTP 401**.
- `JSESSIONID + validationKey` devuelve **200**.
- No hay `Authorization` header en las llamadas SAPI de este flujo.

### 3.2 Flujo Mobile Connect (app de escritorio) — OBSERVED (alternativo)

La implementación Rust (`o2cloud-cli`) usa un flujo distinto, propio de la app
de escritorio/móvil (user-agent `QtWebEngine`, `platform=macos`):

1. `POST /sapi/login/mobileconnect?action=start` (form `platform`, `msisdn`)
   → `authorizationurl`.
2. WebView → intercepta `code`+`state` en `clientoauth.html`.
3. `POST /sapi/credential/mobileconnect?action=validate` (JSON
   `{"data":{"code","state"}}`) → `access_token`, `expires_in`, `msisdn`,
   `platform`, `lastrefreshdate`.
4. `POST /sapi/login?action=login&responsetime=true`
   (header `Authorization: oauth <base64>`) → `access_token`, `jsessionid`,
   `validationkey`, `encryption-token`, `roles`.
5. Las llamadas SAPI envían `Authorization: oauth <base64>` **y** la cookie
   `JSESSIONID`, y permite *silent re-login* reutilizando el `access_token`.

**Decisión:** nuestro proyecto sigue el **flujo web (3.1)** porque es el objetivo
del cliente y el único con `client_id` público documentado.
El flujo 3.2 se documenta como alternativa corroborada pero **no se
implementa**.

### 3.3 Importación manual y keyring

Para el primer vertical slice (sin navegador embebido todavía):

- `o2cloud login --import-cookie '<Cookie: ...>'` captura la cabecera `Cookie`
  de una petición `/sapi/*` en DevTools ("Copy as cURL").
- Debe contener `JSESSIONID` y `validationKey`.
- Se almacena en el keyring del sistema (nunca en texto plano).
- `O2CLOUD_COOKIE` permite testing/automatización explícita.

## 4. Envelopes SAPI

Éxito:

```json
{ "data": { "..." }, "responsetime": 1751441234567 }
```

Error (puede llegar con HTTP 200):

```json
{
  "error": {
    "code": "MED-1000",
    "message": "...",
    "parameters": [],
    "cause": "..."
  },
  "responsetime": 1751441234567
}
```

**Regla:** HTTP 200 **no** implica éxito. Siempre ramificar entre `data` y
`error`. Algunos endpoints (p. ej. el host de subida, `folder?action=save`)
devuelven el objeto directamente sin envelope; hay que tolerarlo.

## 5. Convenciones

- **IDs:** los ids de carpeta llegan como **números JSON**; los de media como
  **strings**. El cliente normaliza ambos a `String`.
- **Fechas de subida:** `YYYYMMDDThhmmssZ` (compacto UTC). Epoch-ms en
  `creationdate`/`modificationdate` provoca `COM-1008`.
- **Tamaños:** bytes (enteros).
- **Consistencia:** eventual tras subir; pollear `get-validation-status`.
- **Descarga:** el campo `url` del detalle es una **ruta firmada relativa al
  host** `/sapi/download/file?action=get&k=<sig>&node=<n>`. Convertir a
  absoluta contra `https://cloud.o2online.es`. El token `k` auto-autentica.

## 6. Tabla de operaciones

| Operación | Método | Endpoint | Request | Response | Evidencia | Estado |
| --------- | ------ | -------- | ------- | -------- | --------- | ------ |
| OIDC authorize (web) | GET | `t3.o2online.es/acceso/...` | query OIDC | redirect `code`+`state` | Python + tráfico | CONFIRMED |
| Code→session (web) | — | `/sapi/login/oauth` | redirect | session cookie | Python (forma UNVERIFIED) | PARTIAL |
| Mobile Connect start | POST | `/sapi/login/mobileconnect?action=start` | form `platform`,`msisdn` | `authorizationurl` | Rust | OBSERVED |
| Credential validate | POST | `/sapi/credential/mobileconnect?action=validate` | `{data:{code,state}}` | `access_token`,… | Rust | OBSERVED |
| Login (desktop) | POST | `/sapi/login?action=login&responsetime=true` | `Authorization: oauth` | tokens | Rust | OBSERVED |
| Quota / almacenamiento | GET | `/sapi/media?action=get-storage-space&softdeleted=true` | — | `{quota,free,used,softdeleted,nolimit,individual}` | Python + AGENTS | CONFIRMED |
| Perfil | GET | `/sapi/profile?action=get` | — | `data` (perfil) | Python + AGENTS | CONFIRMED |
| Perfil (probe) | GET | `/sapi/profile/role?action=get` | — | `data` | Rust | OBSERVED |
| Suscripción | GET | `/sapi/subscription?action=get` | — | `data` | AGENTS | CONFIRMED |
| Plan | GET | `/sapi/subscription/plan?action=get` | — | `data.plans[]` | Python + AGENTS | CONFIRMED |
| Historial | GET | `/sapi/subscription/history?action=get` | — | `data` | AGENTS | CONFIRMED |
| Listar carpetas | POST | `/sapi/media/folder?action=get` | body opcional | `data.folders[]{name,id,status,magic,offline,creationdate,date[,parentid]}` | Live + Python + Rust | CONFIRMED |
| Crear/renombrar carpeta | POST | `/sapi/media/folder?action=save` | `{data:{name[,parentid]}}` | `{data:{folder:{id},success}}` | Live | CONFIRMED |
| Borrar carpeta | POST | `/sapi/media/folder?action=delete` | `{data:{folders:[id]}}` | `data:null` | Live | CONFIRMED |
| Papelera carpeta | POST | `/sapi/media/folder?action=softdelete&id=<id>` | query, sin body | `"Folders have been softdeleted"` | Live | CONFIRMED |
| ZIP carpeta | POST | `/sapi/media/folder?action=zip` | TBD | enlace | AGENTS | UNCONFIRMED |
| Mover item | POST | `/sapi/media/folder?action=add-item` | `{data:{folderid,items:[id]}}` | `data:null` (mueve; el detalle expone `folder`) | Live | CONFIRMED |
| Mover item (remove) | POST | `/sapi/media/folder?action=remove-item` | probable `{folderid,items}` | envelope | — | UNCONFIRMED |
| Listar media (sparse) | GET | `/sapi/media?action=get&limit=N` | — | `data{media[],more}` | Live + Python | CONFIRMED |
| Detalle media | POST | `/sapi/media/file?action=get` | cualquiera (ignora el filtro de id) | `data.files[]` (**TODOS**; filtrar en cliente) | Live | CONFIRMED |
| Detalle por tipo | POST | `/sapi/media/{picture,video,audio}?action=get` | idem | idem | Python | OBSERVED |
| Subida (multipart) | POST | `upload…/sapi/upload?action=save` | multipart `data`+`file` | `{success:"…",id,etag,status,type}` | Live + Python | CONFIRMED |
| Estado validación | POST | `/sapi/media?action=get-validation-status` | `{data:{ids:[{id}]}}` | `data.ids[]{id,status}` | Live + Python | CONFIRMED |
| Descarga | GET | `/sapi/download/file?action=get&k=…&node=…` | — | bytes | Live + Python | CONFIRMED |
| Borrar media (permanente) | POST | `/sapi/media/file?action=delete` | `{data:{files:[id]}}` | `"Files removed successfully"` | Live | CONFIRMED |
| Papelera media | POST | `/sapi/media?action=softdelete&id=<id>` | query, sin body | `"Item successfully moved to trash"` | Live | CONFIRMED |
| Copiar (server-side) | POST | `/sapi/media/file?action=copy` | TBD (`COM-1011` con todos los probados) | — | Live | UNCONFIRMED |
| Revisión get | POST | `/sapi/media/revision?action=get` | TBD | TBD | AGENTS | UNCONFIRMED |
| Revisión restore | POST | `/sapi/media/revision?action=restore` | TBD | TBD | AGENTS | UNCONFIRMED |
| Miniatura | GET | `/sapi/download/thumbnail?action=get&fdoid=<id>` | — | bytes | Python + AGENTS | OBSERVED |
| Papelera listar | GET | `/sapi/media/trash?action=get` | — | `data.entries[]{date,id,name,type}` | Live | CONFIRMED |
| Papelera vaciar | POST | borrado permanente por entrada | — | — | Live | CONFIRMED |
| Papelera restaurar | POST | `/sapi/media/trash?action=…` | TBD | TBD | — | UNCONFIRMED |
| Buscar | — | (sin endpoint confirmado) | — | — | — | UNCONFIRMED |
| Shares/álbumes/labels | — | `/sapi/media/set`, `/sapi/label`, `/sapi/link` | best-effort | — | AGENTS | UNCONFIRMED |

## 7. Detalle por operación (implementadas en el MVP)

### 7.1 Quota — CONFIRMED

```
GET https://cloud.o2online.es/sapi/media?action=get-storage-space&softdeleted=true&validationkey=<vk>
Cookie: JSESSIONID=...; validationKey=<vk>
```

Response `data`:

```json
{
  "quota": 10995116277760,
  "free": 10995116277760,
  "used": 0,
  "softdeleted": 0,
  "nolimit": false,
  "individual": { "used": 0, "softdeleted": 0 }
}
```

### 7.2 Perfil (whoami) — CONFIRMED

```
GET https://cloud.o2online.es/sapi/profile?action=get&validationkey=<vk>
```

Se usa para validar la sesión (401 → sesión caducada) y para `status`.

### 7.3 Listar carpetas — CONFIRMED

```
POST https://cloud.o2online.es/sapi/media/folder?action=get&validationkey=<vk>
```

Response `data.folders[]` (el `id` es número; la impl. Rust además observa
`parentid` para construir el árbol):

```json
{
  "folders": [
    { "name": "/", "id": 1, "status": "V", "magic": false, "offline": false, "date": 1751441234567 },
    { "name": "Fotos", "id": 2, "magic": true, ... }
  ]
}
```

### 7.4 Listar media (sparse) — CONFIRMED

```
GET https://cloud.o2online.es/sapi/media?action=get&limit=100&validationkey=<vk>
```

Response `data{media[], more}`. Elementos sparse: `{id, date, mediatype,
status, userid}`. `id` es **string**. El parámetro opcional `folder=<id>` se
reenvía (comportamiento de filtrado no confirmado al 100%).

### 7.5 Detalle media — CONFIRMED (validado live 2026-10-07)

```
POST https://cloud.o2online.es/sapi/media/file?action=get&validationkey=<vk>
{"data":{"files":[{"id":"123"}]}}   # el id es IGNORADO por el servidor
```

**Confirmado en vivo**: el endpoint devuelve el detalle de **todos** los
archivos (ignora el filtro de `id`); el cliente debe filtrar por id en local.

Campos por archivo (valores reales observados):

```json
{
  "id": "1501990478", "date": 1791387814407,
  "creationdate": 1791387808000,      // epoch-ms (entero), NO cadena
  "modificationdate": 1791387808000,  // epoch-ms (entero)
  "uploaded": 1791387808296,          // epoch-ms (entero), NO bool
  "size": 1443, "name": "o2upload.txt", "mediatype": "file",
  "status": "U", "etag": "…", "softdeleted": false,
  "folder": 34562759,                 // carpeta contenedora (None = raíz)
  "url": "/sapi/download/file?action=get&k=…&node=…"
}
```

> **No** usar `POST /sapi/media?action=get` con `{"data":{"ids":[{"id":…}]}}`:
> devuelve `MED-1000`.

## 8. Errores conocidos

| Código | Significado | Mapeo sugerido |
| ------ | ----------- | -------------- |
| HTTP 401/403 | sesión inválida/expirada | `Unauthorized` |
| HTTP 429 | rate limit | `RateLimited` |
| HTTP 404 | recurso inexistente | `NotFound` |
| HTTP 5xx | error de servidor | `Server` |
| `MED-1000` | media no encontrado / petición incorrecta | `NotFound` |
| `COM-1008` | fecha de subida en formato incorrecto | `Protocol`/`Server` |
| `SEC-*`, `AUT-*` | seguridad/autorización | `Unauthorized` |
| `QUO-*` | cuota | `QuotaExceeded` |

## 9. Discrepancias entre implementaciones

| Área | Python (web) | Rust (escritorio) | Resolución |
| ---- | ------------ | ----------------- | ---------- |
| Backend | Funambol OneMediaHub | Synchronoss OneMediaHub v31 | Mismo producto (Synchronoss compró Funambol) |
| Auth | OIDC web: `JSESSIONID`+`validationKey` (sin `Authorization`) | Mobile Connect + `Authorization: oauth` + silent refresh | Seguimos el flujo web |
| Listar media | `GET` sparse + detalle aparte | `POST` con `fields` (incl. `url`,`folderid`) | Seguimos `GET` sparse; el `POST` con `fields` queda OBSERVED |
| Subida | multipart `data`+`file` a `/sapi/upload?action=save` | 2 pasos `save-metadata` + `save` (octet-stream) | Seguimos multipart (CONFIRMED live) |
| Borrar media | `media/file?action=delete` body `files:[id]` (permanente) y `media/file?action=softdelete` body (papelera, **no-op real**) | `media?action=softdelete&id=<id>` (papelera) | Papelera = **query `id`**; permanente = **body `files:[id]`** (validado live) |
| Borrar carpeta | body `folders:[id]` | query `id=<id>` | Permanente = **body `folders:[id]`**; papelera = **query `id`** (validado live) |
| mkdir body | `{name, parent}` (**`parent` ignorado**) | `{name, parentid, …}` | Usar **`parentid`**; respuesta real `{folder:{id},success}` (validado live) |
| Mover item | `{id, items:[id]}` (**COM-1011**) | `add-item` | Usar **`{folderid, items:[id]}`** (validado live) |
| Detalle media | `{"files":[{"id":…}]}`, toma `files[0]` | `POST /media?action=get` con `fields` | El endpoint devuelve **todos** los archivos; filtrar por id en cliente (validado live) |
| Copiar | `media/file?action=copy` body `files:[id][,folder]` | — | **UNCONFIRMED** (`COM-1011` con todos los cuerpos probados) |
| Probe de sesión | `profile?action=get` | `profile/role?action=get` | Usamos `profile?action=get` |

## 10. Validación live (2026-10-07)

Contrastado contra una cuenta real con sesión web válida. Correcciones
respecto a lo deducido de las implementaciones de referencia:

- **`mkdir`**: el cuerpo usa **`parentid`** (no `parent`, que se ignora) y la
  respuesta es `{"data":{"folder":{"id":…},"success":"…"}}`.
- **Detalle** `media/file?action=get`: devuelve **todos** los archivos (ignora
  el id); `creationdate`/`modificationdate`/`uploaded` son **epoch-ms enteros**;
  no hay `contenttype`; incluye **`folder`** (carpeta contenedora; ausente/null
  = raíz).
- **Listado de carpetas**: incluye `parentid` (a veces omitido en el primer
  nivel).
- **`move`**: `add-item` con **`{"data":{"folderid":…,"items":[id]}}`** mueve el
  elemento y el detalle pasa a exponer `folder`.
- **Borrado**: papelera de media = `POST /media?action=softdelete&id=<id>`;
  papelera de carpeta = `POST /media/folder?action=softdelete&id=<id>`;
  permanente de media = `POST /media/file?action=delete` body
  `{"data":{"files":[id]}}`; permanente de carpeta =
  `POST /media/folder?action=delete` body `{"data":{"folders":[id]}}`.
  El `media/file?action=softdelete` **con body es un no-op**.
- **Subida**: `success` llega como **cadena**; la carpeta destino se indica en
  **`folderid`** (la clave `folder` que usa el cliente web se **ignora** y el
  archivo cae en la raíz).
- **`cp`/copy**: **UNCONFIRMED** (`COM-1011` con todos los cuerpos probados).
- **Papelera**: listar con `GET /sapi/media/trash?action=get` →
  `data.entries[]{date,id,name,type}` (`type`: `media`|`folder`). El vaciado se
  hace borrando **permanentemente** cada entrada (no hay endpoint masivo
  confirmado). Restaurar sigue sin confirmar.

## 11. Áreas no confirmadas (no implementar sin captura live)

- Búsqueda server-side (sin endpoint).
- Shares, álbumes, labels.
- Restaurar de papelera (el listado y el vaciado ya están confirmados).
- Parámetros de `export`/`zip`.
- `cp`/copy server-side (cuerpo del endpoint desconocido; `COM-1011`).
- `remove-item` y borrado recursivo de carpetas.
- Forma exacta del exchange code→session del flujo web.
- OAuth bearer / silent refresh del móvil.
