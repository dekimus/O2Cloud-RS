//! Transporte HTTP SAPI: `O2CloudClient`.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, USER_AGENT};
use reqwest::{Client, Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::auth::session::Session;
use crate::config::AppConfig;
use crate::error::{Error, Result};
use crate::progress::ProgressReporter;
use crate::util::absolutize;

/// Cliente de alto nivel para el SAPI de O2 Cloud.
///
/// Responsabilidades separadas:
/// - transporte (`reqwest`, TLS verificado, timeouts);
/// - sesión (adjunta `Cookie` y `?validationkey=`);
/// - parseo de envelopes (`data` vs `error`);
/// - operaciones (delegadas a los módulos `media`, `folders`, `quota`, `auth`).
#[derive(Debug)]
pub struct O2CloudClient {
    http: Client,
    transfer_http: Client,
    base_url: String,
    upload_base_url: String,
    device_id: String,
    session: Option<Session>,
}

impl O2CloudClient {
    /// Crea un cliente contra la configuración dada, con una sesión opcional.
    pub fn new(config: &AppConfig, session: Option<Session>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(config.user_agent())
                .map_err(|_| Error::Protocol("invalid user-agent".to_string()))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        // Cliente de metadatos: timeout total corto.
        let http = Client::builder()
            .default_headers(headers.clone())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(same_host_redirect_policy())
            .build()?;

        // Cliente de transferencias (subida/descarga): sin timeout total, solo
        // timeout de conexión — un archivo grande puede tardar mucho.
        let transfer_http = Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(15))
            .redirect(same_host_redirect_policy())
            .build()?;

        Ok(Self {
            http,
            transfer_http,
            base_url: config.base_url().to_string(),
            upload_base_url: config.upload_base_url().to_string(),
            device_id: config.device_id().to_string(),
            session,
        })
    }

    /// Sesión activa (si la hay).
    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Identificador de dispositivo (cabeceras de subida).
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Acceso a las operaciones de media.
    #[must_use]
    pub fn media(&self) -> crate::api::media::MediaApi<'_> {
        crate::api::media::MediaApi { client: self }
    }

    /// Acceso a las operaciones de carpetas.
    #[must_use]
    pub fn folders(&self) -> crate::api::folders::FoldersApi<'_> {
        crate::api::folders::FoldersApi { client: self }
    }

    /// Acceso a las operaciones de cuota.
    #[must_use]
    pub fn quota(&self) -> crate::api::quota::QuotaApi<'_> {
        crate::api::quota::QuotaApi { client: self }
    }

    /// Acceso a las operaciones de subida.
    #[must_use]
    pub fn upload(&self) -> crate::api::upload::UploadApi<'_> {
        crate::api::upload::UploadApi { client: self }
    }

    /// Acceso a las operaciones de sesión/perfil.
    #[must_use]
    pub fn auth(&self) -> crate::api::auth::AuthApi<'_> {
        crate::api::auth::AuthApi { client: self }
    }

    /// GET idempotente (con reintentos prudentes sobre errores de transporte).
    pub async fn get<T>(&self, path: &str, params: &[(&str, &str)]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request(Method::GET, path, params, None, true).await
    }

    /// POST de mutación (sin reintentos ciegos).
    pub async fn post<T>(
        &self,
        path: &str,
        params: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request(Method::POST, path, params, body, false).await
    }

    /// POST de lectura idempotente (p. ej. `media/file?action=get`).
    pub async fn post_read<T>(
        &self,
        path: &str,
        params: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.request(Method::POST, path, params, body, true).await
    }

    async fn request<T>(
        &self,
        method: Method,
        path: &str,
        params: &[(&str, &str)],
        body: Option<&Value>,
        idempotent: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.build_url(path, params)?;

        // Reintentos: solo peticiones idempotentes y solo sobre errores de
        // transporte (conexión/timeout), nunca sobre respuestas HTTP.
        let max_attempts = if idempotent { 3 } else { 1 };
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.send_once(method.clone(), &url, body).await {
                Err(Error::Transport(_)) if attempt < max_attempts => {
                    tokio::time::sleep(Duration::from_millis(200 * (1 << attempt))).await;
                }
                other => return other,
            }
        }
    }

    async fn send_once<T>(&self, method: Method, url: &Url, body: Option<&Value>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        tracing::debug!(method = %method, path = %url.path(), "request");
        let mut req = self.http.request(method, url.clone());
        if let Some(session) = &self.session {
            req = req.header(COOKIE, session.cookie_header());
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await?;
        Self::parse(resp).await
    }

    fn build_url(&self, path: &str, params: &[(&str, &str)]) -> Result<Url> {
        self.build_url_with(&self.base_url, path, params)
    }

    fn build_url_with(&self, base: &str, path: &str, params: &[(&str, &str)]) -> Result<Url> {
        let base = format!("{base}/sapi{path}");
        let mut url =
            Url::parse(&base).map_err(|e| Error::Protocol(format!("invalid URL: {e}")))?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
            if let Some(session) = &self.session {
                query.append_pair("validationkey", session.validation_key());
            }
        }
        Ok(url)
    }

    /// Envía un formulario multipart al host de subida.
    ///
    /// Usado por [`crate::api::upload::UploadApi`]; no reintenta (la subida no
    /// es idempotente sin reconciliación previa).
    pub(crate) async fn post_multipart<T>(
        &self,
        path: &str,
        params: &[(&str, &str)],
        form: reqwest::multipart::Form,
        extra_headers: &[(&str, String)],
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.build_url_with(&self.upload_base_url, path, params)?;
        tracing::debug!(path = %url.path(), "upload request");

        let mut req = self.transfer_http.post(url);
        if let Some(session) = &self.session {
            req = req.header(COOKIE, session.cookie_header());
        }
        for (name, value) in extra_headers {
            req = req.header(*name, value);
        }
        let resp = req.multipart(form).send().await?;
        Self::parse(resp).await
    }

    /// Parsea una respuesta SAPI: distingue error HTTP, error SAPI (dentro de
    /// HTTP 200), éxito (`data`) y respuesta inválida.
    async fn parse<T: DeserializeOwned>(resp: Response) -> Result<T> {
        let status = resp.status();

        // Estados HTTP que mandan sobre la forma del cuerpo.
        if let Some(err) = status_error(status) {
            return Err(err);
        }

        let text = resp.text().await?;

        let value = if text.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => value,
                Err(_) => {
                    return Err(Error::InvalidResponse(format!(
                        "non-JSON response (status {status})"
                    )));
                }
            }
        };

        // Envelope de error (puede llegar con HTTP 200).
        if let Some(err) = value.get("error").filter(|e| !e.is_null()) {
            return Err(map_sapi_error(err));
        }

        // Éxito: desenvolver `data` si existe; si no, devolver el objeto tal
        // cual (algunos endpoints no usan envelope).
        let data = match value {
            Value::Object(mut map) => map.remove("data").unwrap_or(Value::Object(map)),
            other => other,
        };

        serde_json::from_value(data)
            .map_err(|e| Error::InvalidResponse(format!("unexpected response shape: {e}")))
    }

    /// Descarga el contenido de una URL firmada, en streaming, al fichero
    /// `dest`.
    ///
    /// Convierte la ruta relativa `/sapi/...` en absoluta contra el host
    /// principal, aplica la cookie de sesión (`k` auto-autentica) y escribe
    /// por trozos sin cargar el archivo en memoria. Si falla, elimina el
    /// fichero parcial.
    pub(crate) async fn download_signed_url(
        &self,
        signed_url: &str,
        dest: &Path,
        progress: &ProgressReporter,
    ) -> Result<u64> {
        let absolute = absolutize(&self.base_url, signed_url);
        let base = Url::parse(&self.base_url)
            .map_err(|e| Error::Protocol(format!("invalid base URL: {e}")))?;
        let url = Url::parse(&absolute)
            .map_err(|e| Error::Protocol(format!("invalid signed URL: {e}")))?;

        // Evitar SSRF / fuga de cookies: la URL firmada debe ser del mismo host
        // y esquema que el configurado (nunca seguir un `url` absoluto ajeno).
        if url.host_str() != base.host_str() || url.scheme() != base.scheme() {
            return Err(Error::Protocol(
                "refusing to download from a different host".to_string(),
            ));
        }

        tracing::debug!(path = %url.path(), "download request");

        let mut req = self.transfer_http.get(url);
        if let Some(session) = &self.session {
            req = req.header(COOKIE, session.cookie_header());
        }
        let resp = req.send().await?;

        let status = resp.status();
        if let Some(err) = status_error(status) {
            return Err(err);
        }

        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(|e| Error::LocalIo(format!("cannot create {}: {e}", dest.display())))?;

        let mut stream = resp.bytes_stream();
        let mut total: u64 = 0;
        loop {
            let next = stream.next().await;
            match next {
                Some(Ok(chunk)) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        remove_file_quietly(dest).await;
                        return Err(Error::LocalIo(format!(
                            "cannot write {}: {e}",
                            dest.display()
                        )));
                    }
                    total += chunk.len() as u64;
                    progress.inc(chunk.len() as u64);
                }
                Some(Err(e)) => {
                    remove_file_quietly(dest).await;
                    return Err(e.into());
                }
                None => break,
            }
        }

        if let Err(e) = file.flush().await {
            remove_file_quietly(dest).await;
            return Err(Error::LocalIo(format!(
                "cannot flush {}: {e}",
                dest.display()
            )));
        }
        if let Err(e) = file.sync_all().await {
            remove_file_quietly(dest).await;
            return Err(Error::LocalIo(format!(
                "cannot sync {}: {e}",
                dest.display()
            )));
        }

        Ok(total)
    }
}

/// Elimina un fichero ignorando errores (limpieza de parciales).
async fn remove_file_quietly(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

/// Política de redirecciones: solo permite seguir al **mismo host** que el
/// original (evita SSRF vía `Location` a un host ajeno). Reqwest además elimina
/// las cabeceras sensibles en redirecciones cross-host por defecto.
fn same_host_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let same_host = attempt
            .previous()
            .first()
            .and_then(|url| url.host_str())
            .is_some_and(|origin| attempt.url().host_str() == Some(origin));
        if same_host {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

/// Mapea un estado HTTP a un error tipado (o `None` si es 2xx).
fn status_error(status: StatusCode) -> Option<Error> {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        Some(Error::Unauthorized(
            "session expired or invalid — run 'o2cloud login' again".to_string(),
        ))
    } else if status == StatusCode::NOT_FOUND {
        Some(Error::NotFound("resource not found".to_string()))
    } else if status == StatusCode::CONFLICT {
        Some(Error::Conflict("resource conflict".to_string()))
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        Some(Error::RateLimited("rate limited by SAPI".to_string()))
    } else if status.is_server_error() {
        Some(Error::Server(format!("server error (HTTP {status})")))
    } else {
        None
    }
}

/// Mapea un envelope de error SAPI a un error tipado.
fn map_sapi_error(err: &Value) -> Error {
    let code = err
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let prefix = code
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    match prefix.as_str() {
        "SEC" | "AUT" => Error::Unauthorized(format!("{code}: {message}")),
        "QUO" => Error::QuotaExceeded(format!("{code}: {message}")),
        _ if code == "MED-1000" => Error::NotFound(format!("{code}: {message}")),
        _ => Error::Sapi { code, message },
    }
}
