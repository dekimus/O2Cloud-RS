//! Utilidades de texto, formato y redacción de secretos.

/// Redacta fragmentos sensibles (`validationkey=…`, `JSESSIONID=…`, firmas
/// `k=…`, tokens…) de una cadena arbitraria, para poder escribir logs y errores
/// sin filtrar secretos.
///
/// Solo considera la clave cuando va al inicio o precedida de un delimitador de
/// parámetro/cookie (`?`, `&`, `;`, espacio, …), para no enmascarar palabras que
/// contengan la clave (p. ej. `link=` no debe redactarse).
#[must_use]
pub fn redact_secrets(input: &str) -> String {
    const SENSITIVE_KEYS: &[&str] = &[
        "validationkey",
        "jsessionid",
        "access_token",
        "accesstoken",
        "refreshtoken",
        "encryption-token",
        "signature",
        "token",
        "pat",
        "k",
    ];

    let mut out = input.to_string();
    for key in SENSITIVE_KEYS {
        redact_key(&mut out, key);
    }
    out
}

fn redact_key(out: &mut String, key: &str) {
    let mut search_start = 0usize;
    while let Some(rel) = find_ci_from(out, key, search_start) {
        let after_key = rel + key.len();

        if !is_param_boundary(out, rel) {
            search_start = after_key;
            continue;
        }

        // Saltar espacios en blanco entre la clave y el `=`.
        let mut eq = after_key;
        while eq < out.len() && out.as_bytes()[eq] == b' ' {
            eq += 1;
        }
        if eq >= out.len() || out.as_bytes()[eq] != b'=' {
            search_start = after_key;
            continue;
        }

        let value_start = eq + 1;
        let value_end = out[value_start..]
            .find(|c: char| ['&', ' ', ';', '"', '\'', '\n', '\r', ',', '}', '|'].contains(&c))
            .map_or(out.len(), |p| value_start + p);

        out.replace_range(value_start..value_end, "[REDACTED]");
        search_start = value_start + "[REDACTED]".len();
    }
}

/// `true` si la posición `pos` es un inicio o un delimitador de clave.
fn is_param_boundary(s: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    matches!(
        s.as_bytes()[pos - 1],
        b'?' | b'&'
            | b';'
            | b' '
            | b'\t'
            | b','
            | b'"'
            | b'\''
            | b'('
            | b'['
            | b'{'
            | b'\n'
            | b'\r'
            | b'|'
    )
}

/// Busca `needle` en `haystack` a partir del byte `from`, ignorando
/// mayúsculas/minúsculas (solo ASCII). Devuelve el offset de byte del primer
/// match.
#[must_use]
pub fn find_ci_from(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || from >= h.len() {
        return None;
    }
    h[from..]
        .windows(n.len())
        .position(|window| {
            window
                .iter()
                .zip(n)
                .all(|(&a, &b)| a.eq_ignore_ascii_case(&b))
        })
        .map(|pos| from + pos)
}

/// `true` si `s` es exactamente 32 caracteres hexadecimales.
#[must_use]
pub fn is_hex32(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Convierte una ruta firmada relativa (`/sapi/...`) en URL absoluta contra
/// `base_url`. Si ya es absoluta (`http`/`https`), la devuelve sin tocar.
#[must_use]
pub fn absolutize(base_url: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if let Some(rest) = url.strip_prefix('/') {
        format!("{}/{}", base_url.trim_end_matches('/'), rest)
    } else {
        format!("{}/{}", base_url.trim_end_matches('/'), url)
    }
}

/// Instante actual formateado como `YYYYMMDDThhmmssZ` (UTC compacto).
///
/// Es el formato que exige el backend en `creationdate`/`modificationdate`;
/// epoch-ms se rechaza con `COM-1008`.
#[must_use]
pub fn compact_utc_now() -> String {
    format_compact_utc(time::OffsetDateTime::now_utc())
}

/// Formatea un `OffsetDateTime` como `YYYYMMDDThhmmssZ`.
#[must_use]
pub fn format_compact_utc(dt: time::OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// Formatea un tamaño en bytes de forma legible (unidades binarias).
#[must_use]
#[allow(clippy::cast_precision_loss)] // formato legible: la precisión f64 sobra.
pub fn format_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_validationkey_query() {
        let out = redact_secrets("https://cloud.o2online.es/sapi/media?action=get&validationkey=0123456789abcdef0123456789abcdef");
        assert!(out.contains("validationkey=[REDACTED]"));
        assert!(!out.contains("0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn redacts_validationkey_cookie_case_insensitive() {
        let out = redact_secrets(
            "Cookie: JSESSIONID=abc123; validationKey=deadbeefdeadbeefdeadbeefdeadbeef",
        );
        assert!(out.contains("validationKey=[REDACTED]"));
        assert!(out.contains("JSESSIONID=[REDACTED]"));
        assert!(!out.contains("abc123"));
        assert!(!out.contains("deadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn leaves_innocent_strings_untouched() {
        let s = "GET /sapi/media?action=get — ok";
        assert_eq!(redact_secrets(s), s);
    }

    #[test]
    fn redacts_signed_download_signature() {
        let out = redact_secrets("/sapi/download/file?action=get&k=GSw6PaXbsoCd&node=2i196");
        assert!(out.contains("k=[REDACTED]"), "{out}");
        assert!(!out.contains("GSw6PaXbsoCd"), "{out}");
        assert!(out.contains("node=2i196"), "{out}");
    }

    #[test]
    fn does_not_redact_key_inside_words() {
        let s = "link=https://example/x; back=1";
        assert_eq!(redact_secrets(s), s);
    }

    #[test]
    fn is_hex32_detects_valid_and_invalid() {
        assert!(is_hex32("0123456789abcdef0123456789abcdef"));
        assert!(!is_hex32("xyz"));
        assert!(!is_hex32("0123456789abcdef0123456789abcde"));
    }

    #[test]
    fn find_ci_from_ignores_case() {
        assert_eq!(find_ci_from("abcDEFghi", "def", 0), Some(3));
        assert_eq!(find_ci_from("abcdef", "X", 0), None);
        assert_eq!(find_ci_from("abABab", "ab", 2), Some(2));
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.0 TiB");
    }

    #[test]
    fn formats_compact_utc() {
        let dt = time::OffsetDateTime::from_unix_timestamp(1_705_314_645).unwrap();
        assert_eq!(format_compact_utc(dt), "20240115T103045Z");
    }

    #[test]
    fn absolutizes_signed_paths() {
        assert_eq!(
            absolutize(
                "https://cloud.o2online.es",
                "/sapi/download/file?action=get&k=x"
            ),
            "https://cloud.o2online.es/sapi/download/file?action=get&k=x"
        );
        assert_eq!(
            absolutize("https://cloud.o2online.es/", "/sapi/download/file"),
            "https://cloud.o2online.es/sapi/download/file"
        );
        assert_eq!(
            absolutize("https://cloud.o2online.es", "https://other.example/x"),
            "https://other.example/x"
        );
    }
}
