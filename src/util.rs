//! Utilidades de texto, formato y redacción de secretos.

/// Redacta fragmentos sensibles (`validationkey=...`, `JSESSIONID=...`) de una
/// cadena arbitraria, para poder escribir logs y errores sin filtrar secretos.
///
/// Busca las claves de forma insensible a mayúsculas y enmascara el valor que
/// les sigue hasta el siguiente delimitador (`&`, espacio, `;`, comillas, …).
#[must_use]
pub fn redact_secrets(input: &str) -> String {
    const SENSITIVE_KEYS: &[&str] = &["validationkey", "jsessionid"];
    let mut out = input.to_string();

    for key in SENSITIVE_KEYS {
        let mut search_start = 0usize;
        while let Some(rel) = find_ci_from(&out, key, search_start) {
            let after_key = rel + key.len();

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
                .find(|c: char| ['&', ' ', ';', '"', '\'', '\n', '\r', ',', '}'].contains(&c))
                .map_or(out.len(), |p| value_start + p);

            out.replace_range(value_start..value_end, "[REDACTED]");
            search_start = value_start + "[REDACTED]".len();
        }
    }

    out
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
}
