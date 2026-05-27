use std::collections::HashMap;

pub fn parse_cookies(header: Option<&str>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let header = match header {
        Some(h) => h,
        None => return result,
    };

    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(index) = trimmed.find('=') {
            let key = trimmed[..index].trim().to_string();
            let value = trimmed[index + 1..].trim().to_string();
            if !key.is_empty() {
                result.insert(key, urlencoding_decode(&value));
            }
        }
    }

    result
}

fn urlencoding_decode(value: &str) -> String {
    // Simple percent-decoding for cookies
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .to_string()
}

pub fn is_loopback_address(remote_addr: Option<&str>) -> bool {
    match remote_addr {
        Some("127.0.0.1") | Some("::1") | Some("::ffff:127.0.0.1") => true,
        _ => false,
    }
}

pub fn wants_html(accept_header: Option<&str>) -> bool {
    let accept = accept_header.unwrap_or("").to_lowercase();
    accept.contains("text/html") || accept.contains("application/xhtml")
}

pub fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "::1" || host.starts_with("127.")
}
