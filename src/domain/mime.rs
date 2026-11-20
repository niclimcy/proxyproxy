pub fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "json" => "application/json",
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript",
        "css" => "text/css",
        "xml" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_by_extension() {
        assert_eq!(content_type("x.JSON"), "application/json");
        assert_eq!(content_type("x.unknown"), "application/octet-stream");
    }
}
