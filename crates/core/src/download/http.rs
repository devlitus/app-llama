//! Lógica pura de `Range`/`Content-Range` y clasificación de respuestas.
//! Las cabeceras vienen de un servidor externo (la CDN): se parsean de forma
//! estricta y nunca provocan un `panic`.

use super::model::FailureKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    pub end: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cabecera Content-Range no válida")]
pub struct InvalidContentRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseInfo<'a> {
    pub status: u16,
    pub content_range: Option<&'a str>,
    pub from_hf_origin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseAction {
    Append,
    RestartFromZero,
    AlreadyComplete,
    Fail(FailureKind),
    Retry,
}

pub fn range_header(offset: u64) -> Option<String> {
    (offset > 0).then(|| format!("bytes={offset}-"))
}

/// Acepta solo `bytes <inicio>-<fin>/<total|*>` con dígitos ASCII, sin
/// espacios ni signos, `inicio <= fin` y, si hay total, `fin < total`.
pub fn parse_content_range(value: &str) -> Result<ContentRange, InvalidContentRange> {
    let spec = value.strip_prefix("bytes ").ok_or(InvalidContentRange)?;
    let (span, total) = spec.split_once('/').ok_or(InvalidContentRange)?;
    let (start, end) = span.split_once('-').ok_or(InvalidContentRange)?;
    let start = parse_digits(start)?;
    let end = parse_digits(end)?;
    let total = match total {
        "*" => None,
        digits => Some(parse_digits(digits)?),
    };
    let coherent = start <= end && total.is_none_or(|total| end < total);
    coherent
        .then_some(ContentRange { start, end, total })
        .ok_or(InvalidContentRange)
}

fn parse_digits(text: &str) -> Result<u64, InvalidContentRange> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(InvalidContentRange);
    }
    text.parse().map_err(|_| InvalidContentRange)
}

pub fn classify(info: ResponseInfo<'_>, offset: u64, expected: u64) -> ResponseAction {
    match info.status {
        200 if offset > 0 => ResponseAction::RestartFromZero,
        200 => ResponseAction::Append,
        206 => classify_partial(info.content_range, offset, expected),
        416 if offset == expected => ResponseAction::AlreadyComplete,
        // Con offset 0 no se envía `Range`: reiniciar desde cero repetiría la
        // misma petición y el 416 se encadenaría sin fin.
        416 if offset == 0 => ResponseAction::Fail(FailureKind::UnexpectedContentRange),
        416 => ResponseAction::RestartFromZero,
        401 if info.from_hf_origin => ResponseAction::Fail(FailureKind::Unauthorized),
        403 if info.from_hf_origin => ResponseAction::Fail(FailureKind::Gated),
        // Fuera del origen de HF es la firma caducada de la CDN: el siguiente
        // intento vuelve a pedir `/resolve/` y obtiene una URL nueva.
        401 | 403 | 429 | 500..=599 => ResponseAction::Retry,
        404 => ResponseAction::Fail(FailureKind::NotFound),
        _ => ResponseAction::Fail(FailureKind::Network),
    }
}

fn classify_partial(content_range: Option<&str>, offset: u64, expected: u64) -> ResponseAction {
    let range = content_range.and_then(|value| parse_content_range(value).ok());
    let Some(ContentRange {
        start,
        total: Some(total),
        ..
    }) = range
    else {
        return ResponseAction::Fail(FailureKind::UnexpectedContentRange);
    };
    if start != offset {
        return ResponseAction::Fail(FailureKind::UnexpectedContentRange);
    }
    if total != expected {
        return ResponseAction::Fail(FailureKind::SizeMismatch {
            expected,
            actual: total,
        });
    }
    ResponseAction::Append
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(status: u16, content_range: Option<&str>, from_hf_origin: bool) -> ResponseInfo<'_> {
        ResponseInfo {
            status,
            content_range,
            from_hf_origin,
        }
    }

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn range_offset_cero() {
        assert_eq!(range_header(0), None);
    }

    #[test]
    fn range_offset_grande() {
        assert_eq!(
            range_header(60_000_000_000),
            Some("bytes=60000000000-".to_string())
        );
    }

    #[test]
    fn content_range_valido() {
        assert_eq!(
            parse_content_range("bytes 100-199/1000"),
            Ok(ContentRange {
                start: 100,
                end: 199,
                total: Some(1000)
            })
        );
    }

    #[test]
    fn content_range_total_desconocido() {
        assert_eq!(
            parse_content_range("bytes 100-199/*"),
            Ok(ContentRange {
                start: 100,
                end: 199,
                total: None
            })
        );
    }

    #[test]
    fn content_range_malformado() {
        for value in [
            "bytes=100-199/1000",
            "items 1-2/3",
            "",
            "bytes 200-100/1000",
        ] {
            assert_eq!(
                parse_content_range(value),
                Err(InvalidContentRange),
                "{value:?}"
            );
        }
    }

    #[test]
    fn content_range_rechaza_formas_laxas_y_desbordes() {
        for value in [
            "bytes +1-2/3",
            "bytes 1 -2/3",
            "bytes 1-2/3 ",
            " bytes 1-2/3",
            "bytes 1-2",
            "bytes 1-/3",
            "bytes -2/3",
            "bytes 1-2/",
            "bytes 1-2/3/4",
            "bytes 1-2/-3",
            "bytes 0-5/5",
            "bytes 0-5/3",
            "bytes 18446744073709551616-18446744073709551617/*",
            "bytes 0-18446744073709551616/*",
            "bytes 0-1/18446744073709551616",
            "bytes ٣-٤/5",
        ] {
            assert_eq!(
                parse_content_range(value),
                Err(InvalidContentRange),
                "{value:?}"
            );
        }
    }

    #[test]
    fn content_range_acepta_u64_maximo() {
        assert_eq!(
            parse_content_range("bytes 0-18446744073709551614/18446744073709551615"),
            Ok(ContentRange {
                start: 0,
                end: 18_446_744_073_709_551_614,
                total: Some(u64::MAX)
            })
        );
    }

    #[test]
    fn content_range_mas_de_4gb() {
        assert_eq!(
            parse_content_range("bytes 5000000000-64424509439/64424509440"),
            Ok(ContentRange {
                start: 5_000_000_000,
                end: 64_424_509_439,
                total: Some(64_424_509_440)
            })
        );
    }

    #[test]
    fn classify_206_coherente() {
        let action = classify(info(206, Some("bytes 100-999/1000"), true), 100, 1000);
        assert_eq!(action, ResponseAction::Append);
    }

    #[test]
    fn classify_206_inicio_distinto() {
        let action = classify(info(206, Some("bytes 0-999/1000"), true), 100, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::UnexpectedContentRange)
        );
    }

    #[test]
    fn classify_206_total_distinto() {
        let action = classify(info(206, Some("bytes 100-1199/1200"), true), 100, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::SizeMismatch {
                expected: 1000,
                actual: 1200
            })
        );
    }

    #[test]
    fn classify_206_sin_content_range() {
        let action = classify(info(206, None, true), 100, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::UnexpectedContentRange)
        );
    }

    #[test]
    fn classify_206_content_range_malformado() {
        let action = classify(info(206, Some("bytes=100-999/1000"), true), 100, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::UnexpectedContentRange)
        );
    }

    #[test]
    fn classify_206_total_desconocido() {
        let action = classify(info(206, Some("bytes 100-999/*"), true), 100, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::UnexpectedContentRange)
        );
    }

    #[test]
    fn classify_200_con_offset() {
        let action = classify(info(200, None, true), 4 * MIB, 10 * MIB);
        assert_eq!(action, ResponseAction::RestartFromZero);
    }

    #[test]
    fn classify_200_sin_offset() {
        let action = classify(info(200, None, true), 0, 10 * MIB);
        assert_eq!(action, ResponseAction::Append);
    }

    #[test]
    fn classify_416_completo() {
        let action = classify(info(416, None, true), 1000, 1000);
        assert_eq!(action, ResponseAction::AlreadyComplete);
    }

    #[test]
    fn classify_416_incompleto() {
        let action = classify(info(416, None, true), 500, 1000);
        assert_eq!(action, ResponseAction::RestartFromZero);
    }

    #[test]
    fn classify_416_offset_cero_no_reinicia() {
        let action = classify(info(416, None, true), 0, 1000);
        assert_eq!(
            action,
            ResponseAction::Fail(FailureKind::UnexpectedContentRange)
        );
    }

    #[test]
    fn classify_401_hf() {
        let action = classify(info(401, None, true), 0, 1000);
        assert_eq!(action, ResponseAction::Fail(FailureKind::Unauthorized));
    }

    #[test]
    fn classify_403_hf() {
        let action = classify(info(403, None, true), 0, 1000);
        assert_eq!(action, ResponseAction::Fail(FailureKind::Gated));
    }

    #[test]
    fn classify_403_cdn_reintenta() {
        let action = classify(info(403, None, false), 0, 1000);
        assert_eq!(action, ResponseAction::Retry);
    }

    #[test]
    fn classify_404() {
        let action = classify(info(404, None, true), 0, 1000);
        assert_eq!(action, ResponseAction::Fail(FailureKind::NotFound));
    }

    #[test]
    fn classify_5xx_y_429() {
        for status in [500, 503, 429] {
            let action = classify(info(status, None, true), 0, 1000);
            assert_eq!(action, ResponseAction::Retry, "{status}");
        }
    }

    #[test]
    fn classify_estado_inesperado_falla_sin_reintento() {
        for status in [204, 301, 400, 410] {
            let action = classify(info(status, None, true), 0, 1000);
            assert_eq!(
                action,
                ResponseAction::Fail(FailureKind::Network),
                "{status}"
            );
        }
    }
}
