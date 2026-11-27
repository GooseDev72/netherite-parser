use crate::sources::count::count_links;

pub fn decode_body(body: &str) -> String {
    let b = body.trim();
    if b.is_empty() { return String::new(); }
    // try base64
    if !b.contains("://") {
        if let Ok(bytes) = v2keylib::util::b64_decode(b) {
            if let Ok(text) = String::from_utf8(bytes) {
                if text.contains("://") || text.contains('\n') {
                    return netherite_core::decode::decode_text(&text);
                }
            }
        }
    }
    netherite_core::decode::decode_text(b)
}

pub fn count_from_body(body: &str) -> usize {
    count_links(body)
}
