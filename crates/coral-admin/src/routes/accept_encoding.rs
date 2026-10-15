use std::convert::Infallible;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};

pub const BROTLI: &str = "br";

const QUALITY_PARAM: &str = "q=";
const ZERO_QUALITY: f32 = 0.0;

pub struct AcceptsBrotli(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for AcceptsBrotli {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(accepts_brotli(&parts.headers)))
    }
}

fn accepts_brotli(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(is_acceptable_brotli)
}

fn is_acceptable_brotli(coding: &str) -> bool {
    let mut parts = coding.split(';').map(str::trim);
    let named_brotli = parts
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(BROTLI));
    named_brotli && !parts.any(is_zero_quality)
}

fn is_zero_quality(param: &str) -> bool {
    param
        .strip_prefix(QUALITY_PARAM)
        .and_then(|quality| quality.trim().parse::<f32>().ok())
        .is_some_and(|quality| quality <= ZERO_QUALITY)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(header::ACCEPT_ENCODING, HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn brotli_is_accepted_when_listed_with_a_positive_quality() {
        for value in [
            "br",
            "gzip, deflate, br, zstd",
            "gzip;q=1.0, BR;q=0.5",
            " br ; q=1",
        ] {
            assert!(accepts_brotli(&headers(&[value])), "{value}");
        }
    }

    #[test]
    fn brotli_is_refused_when_absent_or_given_zero_quality() {
        for value in ["gzip, deflate", "br;q=0", "br; q=0.000", "brotli", "*"] {
            assert!(!accepts_brotli(&headers(&[value])), "{value}");
        }
        assert!(!accepts_brotli(&HeaderMap::new()));
    }

    #[test]
    fn every_accept_encoding_header_is_read() {
        assert!(accepts_brotli(&headers(&["gzip", "br"])));
    }
}
