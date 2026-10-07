# AGENTS.md --- Cliente Rust para O2 Cloud

## Objetivo

Construir un cliente nativo para Linux, escrito en Rust, que permita
administrar O2 Cloud de Telefónica España (`https://cloud.o2online.es`)
de forma cómoda y automatizable.

El backend de O2 Cloud no expone una API pública/documentada. La
implementación debe hablar con el SAPI de Funambol OneMediaHub observado
en el portal web. Este proyecto es interoperabilidad no oficial: no
asumir que la API será estable y no inventar endpoints.

El objetivo inicial es una **CLI sólida y una librería Rust
reutilizable**. La arquitectura debe permitir añadir después TUI,
integración con FUSE/rclone o una GUI sin reescribir el cliente HTTP.

## Investigación de partida --- NO volver a empezar desde cero

Existe una implementación pública reciente en Python que ha documentado
y probado el protocolo:

-   `https://github.com/dbaratech/o2cloud`
-   API observada: `https://cloud.o2online.es/sapi/`
-   Referencia:
    `https://github.com/dbaratech/o2cloud/blob/main/docs/api-reference.md`
-   Arquitectura:
    `https://github.com/dbaratech/o2cloud/blob/main/docs/ARCHITECTURE.md`
-   También existe `danielperez9430/o2cloud-cli`, una implementación
    Rust que confirma parte del protocolo.

Usar estas implementaciones como **fuentes de evidencia**, no como
autoridad absoluta. Antes de cambiar una parte del protocolo,
contrastar: 1. `dbaratech/o2cloud` 2. `danielperez9430/o2cloud-cli` 3.
tráfico real del portal O2 Cloud cuando sea necesario.

No copiar código innecesariamente. Reimplementar el protocolo
idiomáticamente en Rust y mantener atribución/licencias cuando
corresponda.

## Hechos técnicos conocidos

### Backend

-   Host principal: `https://cloud.o2online.es`
-   SAPI: `https://cloud.o2online.es/sapi/`
-   Host dedicado de subida: `https://upload.cloud.o2online.es`
-   Backend: Funambol OneMediaHub / SAPI.
-   No existe una API pública oficial conocida.

### Autenticación

El login del portal utiliza OIDC/OAuth2 Authorization Code:

-   IdP: `https://t3.o2online.es/acceso/`
-   `client_name`: `O2CLOUD_WEB`
-   `client_id`: `7f8afae4-30e7-4591-b4a8-4fc08d545d1d`
-   scope: `openid`
-   `acr_values=2`, que fuerza autenticación fuerte/SMS.
-   redirect URI: `https://cloud.o2online.es/sapi/login/oauth`

El SAPI necesita conjuntamente:

-   cookie `JSESSIONID` (httpOnly)
-   cookie `validationKey`
-   parámetro `?validationkey=<valor>` en las llamadas.

`validationKey` por sí solo no basta.

### Estrategia de login recomendada

No intentar automatizar el SMS ni almacenar contraseñas.

Implementar, por este orden:

1.  **Login mediante navegador embebido/system browser**, donde el
    usuario realiza el login y SMS.
2.  Capturar/reutilizar la sesión SAPI resultante, incluyendo
    `JSESSIONID` y `validationKey`.
3.  Guardar secretos en el keyring del sistema.
4.  Ofrecer importación manual de cookies como fallback para desarrollo:
    -   usuario inicia sesión en `cloud.o2online.es`
    -   DevTools → Network → request `/sapi/*` → Copy as cURL
    -   importar el `Cookie` que contenga `JSESSIONID` y
        `validationKey`.

Nunca pedir al usuario que pegue DNI, contraseña o SMS en la CLI.

Si la sesión devuelve HTTP 401, informar de que debe renovarse la
sesión.

### Envelopes SAPI

Éxito:

``` json
{
  "data": {},
  "responsetime": 1234567890
}
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
  "responsetime": 1234567890
}
```

IMPORTANTE: algunos errores llegan con HTTP 200. No determinar éxito
únicamente por el status HTTP; parsear siempre el envelope.

## API conocida

### Cuota

``` text
GET /sapi/media?action=get-storage-space&softdeleted=true
```

Devuelve aproximadamente:

``` text
data.quota
data.free
data.used
data.softdeleted
data.nolimit
data.individual.used
data.individual.softdeleted
```

Los tamaños son bytes.

### Perfil

``` text
GET /sapi/profile?action=get
```

También existen endpoints auxiliares de properties/fields/changes.

### Suscripción

``` text
GET /sapi/subscription?action=get
GET /sapi/subscription/plan?action=get
GET /sapi/subscription/history?action=get
```

### Carpetas

Listar raíz/carpetas:

``` text
POST /sapi/media/folder?action=get
```

Guardar/crear/renombrar:

``` text
POST /sapi/media/folder?action=save
```

Borrar:

``` text
POST /sapi/media/folder?action=delete
```

Papelera:

``` text
POST /sapi/media/folder?action=softdelete
```

ZIP:

``` text
POST /sapi/media/folder?action=zip
```

Mover elementos:

``` text
POST /sapi/media/folder?action=add-item
POST /sapi/media/folder?action=remove-item
```

### Listado de archivos

``` text
GET /sapi/media?action=get&limit=N
```

La respuesta inicial es sparse. Los elementos suelen incluir:

``` text
id
date
mediatype
status
userid
```

Para metadata completa:

``` text
POST /sapi/media/file?action=get
```

Body:

``` json
{
  "data": {
    "files": [
      {"id": 123}
    ]
  }
}
```

NO usar el supuesto:

``` json
{"data":{"ids":[{"id":123}]}}
```

contra `/sapi/media?action=get`; se ha observado error `MED-1000`.

### Descarga

Obtener primero el detalle del archivo. Su campo `url` contiene una ruta
firmada como:

``` text
/sapi/download/file?action=get&k=<signature>&node=<n>
```

Convertirla en URL absoluta contra `https://cloud.o2online.es` y hacer
GET.

El enlace firmado contiene la autorización de descarga. Añadir
`validationkey` según el comportamiento observado.

MUY IMPORTANTE:

-   no concatenar accidentalmente `/sapi/sapi/...`
-   no usar `action=export` como descarga normal; `export` genera
    enlaces/exports.
-   descargar mediante streaming, nunca cargar archivos grandes
    completos en RAM.

### Subida

Host distinto:

``` text
POST https://upload.cloud.o2online.es/sapi/upload?action=save
```

Multipart con dos partes:

1.  `data`: JSON
2.  `file`: bytes

Header:

``` text
X-deviceid: <deviceid>
```

Las fechas `creationdate` y `modificationdate` deben utilizar:

``` text
YYYYMMDDThhmmssZ
```

No epoch milliseconds: se ha observado `COM-1008` cuando se envía el
formato incorrecto.

Respuesta:

``` text
success
id
etag
status
type
```

Después de subir, el archivo puede tardar unos segundos en aparecer en
listados. Implementar polling/backoff mediante:

``` text
POST /sapi/media?action=get-validation-status
```

con:

``` json
{
  "data": {
    "ids": [
      {"id": 123}
    ]
  }
}
```

No asumir consistencia inmediata.

### Borrado

Permanente:

``` text
POST /sapi/media/file?action=delete
```

Body:

``` json
{
  "data": {
    "files": [123, 456]
  }
}
```

Papelera:

``` text
POST /sapi/media/file?action=softdelete
```

Por defecto la CLI debe usar papelera, salvo `--permanent`.

### Copia

Existe copia server-side:

``` text
POST /sapi/media/file?action=copy
```

Preferirla a descargar y volver a subir.

### Versiones

``` text
/sapi/media/revision?action=get
/sapi/media/revision?action=restore
```

Implementación posterior, salvo que sea necesaria para el MVP.

### Miniaturas

``` text
GET /sapi/download/thumbnail?action=get&fdoid=<id>
```

### Papelera

Hay recursos:

``` text
/sapi/media/trash
/sapi/trash/folder
```

Los parámetros exactos de restore/empty deben verificarse antes de
implementarlos.

### Búsqueda

No hay todavía un endpoint de búsqueda inequívocamente confirmado. Para
MVP:

-   implementar búsqueda local sobre listados obtenidos;
-   no inventar un endpoint `/search`;
-   dejar el backend search como tarea posterior.

### Álbumes / shares

Existe superficie:

``` text
/sapi/media/set
/sapi/label
/sapi/link
```

pero parte de ella es best-effort/no completamente capturada.

No bloquear el MVP por shares, álbumes o labels.

## Arquitectura Rust obligatoria

Separar:

``` text
CLI
 ↓
Service / use cases
 ↓
SAPI client
 ↓
HTTP + auth + storage
```

Estructura sugerida:

``` text
src/
  main.rs
  cli/
  service/
  api/
    client.rs
    auth.rs
    media.rs
    folders.rs
    quota.rs
    subscription.rs
    models.rs
    error.rs
  auth/
    browser.rs
    session.rs
    keyring.rs
  storage/
    keyring.rs
    config.rs
  sync/
  output/
  paths.rs
```

### Librerías sugeridas

Evaluar versiones actuales antes de fijarlas:

-   `tokio`
-   `reqwest` con cookies, JSON y streaming
-   `serde`, `serde_json`
-   `clap`
-   `thiserror` o `anyhow`
-   `keyring`
-   `directories`
-   `url`
-   `chrono`/`time`
-   `tracing`, `tracing-subscriber`
-   `indicatif` para progreso
-   `wiremock` o equivalente para tests HTTP
-   `tokio-util` si ayuda con streaming
-   navegador: `wry`/`tao` u otra solución apropiada, pero mantenerla
    desacoplada.

No introducir una dependencia pesada para una necesidad trivial.

## Modelo de errores

Crear errores tipados:

``` text
Authentication
Unauthorized
NotFound
Conflict
QuotaExceeded
RateLimited
Transport
Protocol
Server
InvalidResponse
LocalIo
Cancelled
```

Distinguir:

-   error HTTP
-   error SAPI dentro de HTTP 200
-   error de autenticación
-   error local.

Nunca convertir todo en "authentication failed".

## Seguridad

OBLIGATORIO:

-   TLS siempre verificado.
-   No desactivar certificados.
-   No imprimir cookies, tokens, `validationKey`, URLs firmadas ni
    credenciales.
-   Redactar secretos en logs y errores.
-   Usar keyring para sesiones.
-   No guardar contraseñas en configuración.
-   No guardar cookies en texto plano.
-   Soportar variables de entorno únicamente para testing/automatización
    explícita.
-   Validar destinos locales contra path traversal.
-   Sanitizar nombres recibidos del servidor.
-   No ejecutar comandos de shell.
-   No usar `eval`, serialización insegura ni mecanismos equivalentes.
-   El usuario debe confirmar operaciones destructivas cuando sea
    interactivo.
-   `rm --permanent` debe requerir confirmación o `--yes`.

## Streaming y archivos grandes

10 TB implica que los archivos pueden ser enormes.

OBLIGATORIO:

-   subida streaming/multipart;
-   descarga streaming;
-   nunca `bytes` de todo el archivo;
-   mostrar progreso cuando haya TTY;
-   permitir modo silencioso/JSON para scripts;
-   soportar reintentos sólo cuando sea seguro;
-   evitar reintentar automáticamente una subida completa si no se sabe
    si el servidor ya creó el objeto.

## Consistencia

Asumir consistencia eventual después de upload.

Implementar:

``` text
upload
  -> receive id
  -> validation polling
  -> optional confirmation via listing/detail
```

No declarar "subido y disponible" antes de que el estado lo confirme.

## CLI prevista

MVP:

``` text
o2cloud login
o2cloud logout
o2cloud status
o2cloud quota

o2cloud ls [REMOTE]
o2cloud tree [REMOTE]
o2cloud stat REMOTE

o2cloud mkdir REMOTE
o2cloud upload LOCAL... --to REMOTE
o2cloud download REMOTE... --to LOCAL
o2cloud mv SRC DST
o2cloud cp SRC DST
o2cloud rm REMOTE
```

Después:

``` text
o2cloud sync LOCAL REMOTE
o2cloud trash ...
o2cloud share ...
o2cloud mount ...
```

Opciones globales:

``` text
--json
--verbose
--quiet
--profile NAME
```

Con `--json`:

-   stdout contiene exclusivamente JSON;
-   logs/progreso van a stderr;
-   nunca pedir confirmación;
-   usar códigos de salida estables.

## Pruebas

### Unitarias

Mockear SAPI.

Cubrir:

-   envelope de éxito;
-   envelope de error con HTTP 200;
-   401;
-   cookie + validationKey;
-   listado paginado;
-   metadata;
-   URL firmada;
-   subida multipart;
-   fecha `YYYYMMDDThhmmssZ`;
-   polling de validación;
-   delete/softdelete;
-   move/copy;
-   errores de cuota;
-   rate limiting;
-   streaming;
-   path traversal.

### Integración

No poner credenciales reales en CI.

Permitir un modo manual:

``` text
O2CLOUD_LIVE_TESTS=1
O2CLOUD_COOKIE=...
```

Las pruebas live deben requerir explícitamente una variable de entorno y
nunca ejecutarse por defecto.

No borrar datos reales durante tests salvo que el usuario haya creado
explícitamente un sandbox/test folder.

## Plan de implementación

### Fase 0 --- reconocimiento

1.  Crear repositorio Rust.
2.  Leer este `AGENTS.md`.
3.  Revisar:
    -   `dbaratech/o2cloud/docs/api-reference.md`
    -   `dbaratech/o2cloud/docs/ARCHITECTURE.md`
    -   `danielperez9430/o2cloud-cli`
4.  Crear `docs/protocol.md` con una tabla:
    -   endpoint
    -   método
    -   body
    -   respuesta
    -   estado de evidencia
    -   fuente.
5.  No implementar endpoints no confirmados.

### Fase 1 --- transporte

Implementar:

-   cliente reqwest;
-   cookie jar;
-   validationKey;
-   envelopes;
-   errores;
-   timeouts;
-   retries prudentes;
-   logging redacted.

Probar todo con mocks.

### Fase 2 --- sesión

Implementar:

-   almacenamiento seguro en keyring;
-   importación manual de cookie;
-   detección de 401;
-   interfaz desacoplada para browser login.

Primero hacer funcionar el cliente con una sesión importada. Después
implementar browser-assisted login.

### Fase 3 --- lectura

Implementar:

-   quota;
-   profile;
-   folder listing;
-   media listing;
-   metadata;
-   paths remotos.

### Fase 4 --- mutaciones

En este orden:

1.  download streaming
2.  upload streaming
3.  mkdir
4.  move
5.  copy
6.  soft delete
7.  permanent delete

Después de upload implementar validation polling.

### Fase 5 --- UX

-   progreso;
-   JSON;
-   mensajes claros;
-   conflictos;
-   confirmaciones;
-   perfiles.

### Fase 6 --- sync

Sólo cuando operaciones individuales sean estables.

Diseñar sync con manifests y comparación de:

-   ID remoto;
-   tamaño;
-   fechas;
-   etag;
-   hash local cuando sea necesario.

No asumir que timestamp por sí solo identifica una modificación.

### Fase 7 --- integración Linux

Opcional:

-   TUI;
-   FUSE;
-   integración con `rclone`.

No implementar montaje FUSE hasta que la capa SAPI esté estable.

## Regla de oro para el agente

**No inventes el protocolo.**

Si una llamada no está documentada en las fuentes de evidencia:

1.  marca la operación como `UNCONFIRMED`;
2.  busca en las implementaciones existentes;
3.  si sigue sin estar clara, pide/captura tráfico real;
4.  documenta el request y response;
5.  añade un test;
6.  sólo entonces intégrala.

El objetivo es producir un cliente Rust mantenible, no una colección de
llamadas HTTP que "parecen funcionar".
