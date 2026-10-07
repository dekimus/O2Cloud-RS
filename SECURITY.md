# Seguridad

Cliente de interoperabilidad **no oficial** para O2 Cloud. No está respaldado por
O2/Telefónica. Esta página resume el modelo de seguridad del cliente.

## Secretos

- La sesión (`JSESSIONID` + `validationKey`) se guarda en el **keyring del
  sistema** (o en `O2CLOUD_COOKIE` para testing/automatización explícita).
- **Nunca** se guardan contraseñas ni cookies en texto plano, ni en archivos de
  configuración, ni en el repositorio.
- **Nunca** se imprimen cookies, `validationKey`, tokens ni URLs firmadas. Los
  errores y logs pasan por `redact_secrets`; los logs solo registran método y
  ruta (sin query ni cabeceras).
- El login interactivo (WebView/navegador) no almacena credenciales: solo la
  cookie de sesión resultante.

## Transporte

- TLS **siempre verificado** (`reqwest` + `rustls`, raíces de Mozilla). No hay
  opciones para desactivar la verificación.
- La descarga de URLs firmadas se **restringe al mismo host y esquema**
  configurados (evita SSRF y fugas de cookies).
- No se usa shell ni `eval`; el único proceso externo es el navegador (solo
  `http(s)`, sin intérprete de shell).

## Archivos locales

- Los nombres recibidos del servidor se sanean antes de usarse como ruta
  (anti *path traversal*).
- Las descargas se escriben en `<destino>.part` y se renombran atómicamente; ante
  un fallo se elimina el parcial.
- Subidas y descargas son **streaming** (no se cargan archivos grandes en RAM).

## Reportar vulnerabilidades

Abre un *issue* privado o un aviso de seguridad. No incluyas secretos reales en
los informes; usa `O2CLOUD_COOKIE` de una cuenta de pruebas y descarta la sesión
tras reproducir.

## Recomendaciones de uso

- Evita `login --import-cookie '<cookie>'`: el secreto queda en el historial del
  shell y en `ps`. Prefiere el login por **WebView/navegador**.
- Revisa `docs/security-review.md` para el detalle de la auditoría y los riesgos
  residuales.
