//! Sesión SAPI: credenciales derivadas de la cabecera `Cookie`.

use crate::error::{Error, Result};
use crate::util::{find_ci_from, is_hex32};

/// Sesión SAPI del flujo web.
///
/// Guarda la cabecera `Cookie` completa (que contiene `JSESSIONID` +
/// `validationKey` + cualquier otra cookie) y deriva de ella el
/// `validationKey` (32 hex) que debe viajar además como query param.
#[derive(Debug, Clone)]
pub struct Session {
    cookie: String,
    validation_key: String,
    jsession_id: Option<String>,
}

impl Session {
    /// Construye una sesión a partir de la cabecera `Cookie` capturada en
    /// `DevTools` ("Copy as cURL" → el valor de `-b '…'`).
    ///
    /// Acepta tanto el valor plano de la cabecera como un texto con el prefijo
    /// `Cookie:` o el fragmento `-b '…'` de un cURL.
    pub fn from_cookie_header(raw: &str) -> Result<Self> {
        let cookie = normalize_cookie_header(raw);
        let validation_key = extract_validation_key(&cookie)?;
        let jsession_id = extract_named_cookie(&cookie, "jsessionid");
        Ok(Self {
            cookie,
            validation_key,
            jsession_id,
        })
    }

    /// Cabecera `Cookie` completa a reenviar en cada petición.
    #[must_use]
    pub fn cookie_header(&self) -> &str {
        &self.cookie
    }

    /// `validationKey` (32 hex) a enviar como `?validationkey=`.
    #[must_use]
    pub fn validation_key(&self) -> &str {
        &self.validation_key
    }

    /// `JSESSIONID` si estaba presente en la cookie.
    #[must_use]
    pub fn jsession_id(&self) -> Option<&str> {
        self.jsession_id.as_deref()
    }

    /// `true` si la cookie incluye un `JSESSIONID` (necesario para autenticar).
    #[must_use]
    pub fn has_jsession_id(&self) -> bool {
        self.jsession_id.is_some()
    }
}

/// Normaliza una entrada de cookie: acepta `-b '…'`, el prefijo `Cookie:`, o
/// el valor de cookie directamente.
fn normalize_cookie_header(raw: &str) -> String {
    let mut text = raw.trim();

    // Extraer el valor de un fragmento cURL `-b '...'` / `--cookie "..."`
    if let Some(clause) = extract_curl_cookie(text) {
        text = clause;
    }

    // Quitar un prefijo "Cookie:" inicial.
    if let Some(rest) = strip_case_insensitive_prefix(text, "cookie:") {
        text = rest.trim();
    }

    // Recortar separadores sobrantes.
    text.trim().trim_end_matches(';').trim().to_string()
}

fn extract_curl_cookie(raw: &str) -> Option<&str> {
    for flag in ["-b", "--cookie"] {
        if let Some(pos) = find_ci_from(raw, flag, 0) {
            let after = raw[pos + flag.len()..].trim_start();
            if let Some(quote) = after.chars().next().filter(|c| *c == '\'' || *c == '"') {
                let rest = &after[quote.len_utf8()..];
                if let Some(end) = rest.find(quote) {
                    return Some(&rest[..end]);
                }
            } else if let Some(space) = after.find(' ') {
                return Some(&after[..space]);
            }
        }
    }
    None
}

fn strip_case_insensitive_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Extrae el `validationKey` (32 hex) de una cabecera de cookie o token suelto.
pub fn extract_validation_key(raw: &str) -> Result<String> {
    let text = raw.trim();

    // 1) `validationkey=<hex>` en cualquier parte (case-insensitive).
    if let Some(hex) = find_keyed_hex32(text, "validationkey") {
        return Ok(hex);
    }

    // 2) Por defecto el texto completo; si es `key=value` (sin espacios en la
    //    clave), tomar el valor.
    let mut candidate = text.to_string();
    if let Some((key, value)) = text.split_once('=') {
        let key = key.trim();
        if !key.is_empty() && !key.contains(' ') && !key.contains(';') {
            candidate = value.trim().trim_end_matches(';').trim().to_string();
        }
    }

    let candidate = candidate.to_ascii_lowercase();
    if is_hex32(&candidate) {
        Ok(candidate)
    } else {
        Err(Error::Authentication(
            "no valid 32-hex validationKey found in the supplied value".to_string(),
        ))
    }
}

/// Busca `key=<32hex>` de forma insensible a mayúsculas.
fn find_keyed_hex32(text: &str, key: &str) -> Option<String> {
    let mut search = 0usize;
    while let Some(pos) = find_ci_from(text, key, search) {
        let after = pos + key.len();
        let rest = text[after..].trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            let hex: String = value.chars().take(32).collect();
            if is_hex32(&hex) {
                return Some(hex.to_ascii_lowercase());
            }
        }
        search = after;
    }
    None
}

/// Extrae el valor de una cookie por nombre (case-insensitive) de una cabecera
/// `Cookie: a=1; b=2`.
fn extract_named_cookie(cookie: &str, name: &str) -> Option<String> {
    cookie.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_cookie_header() {
        let s = Session::from_cookie_header(
            "JSESSIONID=abc123; validationKey=0123456789abcdef0123456789abcdef; other=1",
        )
        .unwrap();
        assert_eq!(s.validation_key(), "0123456789abcdef0123456789abcdef");
        assert_eq!(s.jsession_id(), Some("abc123"));
        assert!(s.has_jsession_id());
    }

    #[test]
    fn parses_curl_cookie_clause() {
        let s = Session::from_cookie_header(
            "-b 'JSESSIONID=x; validationKey=0123456789abcdef0123456789abcdef'",
        )
        .unwrap();
        assert_eq!(s.validation_key(), "0123456789abcdef0123456789abcdef");
        assert_eq!(s.jsession_id(), Some("x"));
    }

    #[test]
    fn parses_bare_validation_key() {
        let s =
            Session::from_cookie_header("validationKey=0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(s.validation_key(), "0123456789abcdef0123456789abcdef");
        assert!(!s.has_jsession_id());
    }

    #[test]
    fn rejects_missing_key() {
        assert!(Session::from_cookie_header("JSESSIONID=abc123").is_err());
    }

    #[test]
    fn extracts_validation_key_from_query() {
        assert_eq!(
            extract_validation_key("validationkey=0123456789abcdef0123456789abcdef").unwrap(),
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn extracts_validation_key_bare_token() {
        assert_eq!(
            extract_validation_key("0123456789ABCDEF0123456789ABCDEF").unwrap(),
            "0123456789abcdef0123456789abcdef"
        );
    }
}
