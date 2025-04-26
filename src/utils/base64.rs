use base64::Engine;

pub fn decode_base64(data: &str) -> Result<String, base64::DecodeError> {
    base64::engine::general_purpose::URL_SAFE
        .decode(data)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(data))
        .map(|data| String::from_utf8_lossy(&data).to_string())
}

pub fn encode_base64(data: &str) -> String {
    base64::engine::general_purpose::URL_SAFE.encode(data)
}
