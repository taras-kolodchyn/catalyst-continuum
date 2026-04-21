use tiny_http::{Header, Response, StatusCode};

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
const INDEX_HTML: &str = include_str!("operator_ui/index.html");
const APP_JS: &str = include_str!("operator_ui/app.js");
const STYLES_CSS: &str = include_str!("operator_ui/styles.css");

pub fn route_label(path: &str) -> Option<&'static str> {
    match path {
        "/ui" | "/ui/" => Some("/ui"),
        "/ui/app.js" => Some("/ui/app.js"),
        "/ui/styles.css" => Some("/ui/styles.css"),
        _ => None,
    }
}

pub fn response(path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let (body, content_type) = match path {
        "/ui" | "/ui/" => (INDEX_HTML, "text/html; charset=utf-8"),
        "/ui/app.js" => (APP_JS, "application/javascript; charset=utf-8"),
        "/ui/styles.css" => (STYLES_CSS, "text/css; charset=utf-8"),
        _ => return None,
    };

    Some(static_response(body, content_type))
}

fn static_response(
    body: &'static str,
    content_type: &'static str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(body.as_bytes().to_vec())
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Content-Security-Policy", CONTENT_SECURITY_POLICY))
        .with_header(header("Referrer-Policy", "no-referrer"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
}

fn header(name: &str, value: impl AsRef<str>) -> Header {
    Header::from_bytes(name, value.as_ref())
        .unwrap_or_else(|_| panic!("invalid static UI response header: {name}"))
}

#[cfg(test)]
mod tests {
    use super::{response, route_label};

    #[test]
    fn maps_supported_operator_ui_routes() {
        assert_eq!(route_label("/ui"), Some("/ui"));
        assert_eq!(route_label("/ui/"), Some("/ui"));
        assert_eq!(route_label("/ui/app.js"), Some("/ui/app.js"));
        assert_eq!(route_label("/ui/styles.css"), Some("/ui/styles.css"));
        assert_eq!(route_label("/ui/unknown"), None);
    }

    #[test]
    fn serves_html_with_hardened_headers() {
        let response = response("/ui").expect("operator UI route should resolve");

        assert_eq!(response.status_code().0, 200);
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Type")
                && header.value.as_str() == "text/html; charset=utf-8"
        }));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Cache-Control") && header.value.as_str() == "no-store"
        }));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Security-Policy")
                && header.value.as_str().contains("default-src 'self'")
        }));
    }

    #[test]
    fn serves_javascript_with_specific_content_type() {
        let response = response("/ui/app.js").expect("operator UI app asset should resolve");

        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Type")
                && header.value.as_str() == "application/javascript; charset=utf-8"
        }));
    }
}
