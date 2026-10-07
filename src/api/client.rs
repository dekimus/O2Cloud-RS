//! Transporte HTTP SAPI: `O2CloudClient`.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, USER_AGENT};
use reqwest::{Client, Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::auth::session::Session;
use crate::config::AppConfig;
use crate::error::{Error, Result};

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
            .build()?;

        // Cliente de transferencias (subida/descarga): sin timeout total, solo
        // timeout de conexión — un archivo grande puede tardar mucho.
        let transfer_http = Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(15))
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

        // Estados que mandan sobre la forma del cuerpo.
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::Unauthorized(
                "session expired or invalid — run 'o2cloud login' again".to_string(),
            ));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(Error::NotFound("resource not found".to_string()));
        }
        if status == StatusCode::CONFLICT {
            return Err(Error::Conflict("resource conflict".to_string()));
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::RateLimited("rate limited by SAPI".to_string()));
        }

        let text = resp.text().await?;

        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            if status.is_server_error() {
                return Err(Error::Server(format!("non-JSON {status} response")));
            }
            return Err(Error::InvalidResponse(format!(
                "non-JSON response (status {status})"
            )));
        };

        // Envelope de error (puede llegar con HTTP 200).
        if let Some(err) = value.get("error").filter(|e| !e.is_null()) {
            return Err(map_sapi_error(err));
        }

        if status.is_server_error() {
            return Err(Error::Server(format!("server error (HTTP {status})")));
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
