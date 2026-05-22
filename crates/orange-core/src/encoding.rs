//! Character encoding detection and conversion.

use encoding_rs::Encoding;

/// Detect the encoding of a byte buffer.
pub fn detect_encoding(data: &[u8]) -> &'static Encoding {
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(data, true);
    detector.guess(None, true)
}

/// Decode bytes to UTF-8 string using the detected encoding.
pub fn decode_to_utf8(data: &[u8], encoding: &'static Encoding) -> String {
    let (cow, _encoding_used, had_errors) = encoding.decode(data);
    if had_errors {
        tracing::warn!("Encoding errors detected during decode");
    }
    cow.into_owned()
}
