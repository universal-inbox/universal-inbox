use std::sync::LazyLock;

use actix_files::{Files, NamedFile};
use actix_web::{
    Error, HttpResponse,
    body::MessageBody,
    dev::{HttpServiceFactory, ServiceRequest, ServiceResponse, fn_service},
    http::{
        Method, StatusCode,
        header::{ALLOW, CACHE_CONTROL, HeaderValue},
    },
    middleware::{Next, from_fn},
    web,
};
use regex::Regex;

const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");
const NO_CACHE: HeaderValue = HeaderValue::from_static("no-cache");
const ALLOWED_METHODS: HeaderValue = HeaderValue::from_static("GET, HEAD");

/// Trunk's release build appends a 16 hex digits content hash to the bundle
/// files and the snippets directory (`universal-inbox-web-<hash>_bg.wasm`,
/// `snippets/universal-inbox-web-<hash>/...`).
static CONTENT_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-[0-9a-f]{16}(?:[._/]|$)").unwrap());

/// Serve the single page application from `static_dir`, mounted on `mount`.
///
/// Same behavior as `actix_web_lab::web::spa()` (unknown paths fall back to
/// `index.html`), plus:
/// - precompressed `.br`/`.gz`/`.zst` siblings are served when the client
///   accepts them, so the release bundle is not compressed on every request
///   (the `Compress` middleware skips responses that already have a
///   `Content-Encoding`)
/// - content-hashed files are cached forever, everything else (including
///   `index.html`) is revalidated with its ETag on each use.
pub fn spa_service(mount: &str, static_dir: &str) -> impl HttpServiceFactory + use<> {
    let index_file = format!("{static_dir}/index.html");
    let files = {
        let index_file = index_file.clone();
        Files::new(mount, static_dir)
            .try_compressed()
            // Without an index file, `Files` would try to list directories.
            // This one does not exist, so directories fall to the default handler.
            .index_file("extremely-unlikely-to-exist-!@$%^&*.txt")
            .default_handler(fn_service(move |req| serve_index(req, index_file.clone())))
    };

    web::scope("")
        .wrap(from_fn(set_cache_control))
        .wrap(from_fn(set_allow_header))
        .service(files)
        .default_service(fn_service(move |req| serve_index(req, index_file.clone())))
}

async fn serve_index(req: ServiceRequest, index_file: String) -> Result<ServiceResponse, Error> {
    let (req, _) = req.into_parts();
    // Static content is read-only: `Files` already rejects other methods, the
    // fallback must not answer them with the application page either.
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        let res = HttpResponse::MethodNotAllowed().finish();
        return Ok(ServiceResponse::new(req, res));
    }
    let mut res = NamedFile::open(&index_file)?.into_response(&req);
    res.headers_mut().insert(CACHE_CONTROL, NO_CACHE);
    Ok(ServiceResponse::new(req, res))
}

/// Advertise the methods static content accepts on every `405` response
/// (`Files` answers them without an `Allow` header).
async fn set_allow_header(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let mut res = next.call(req).await?;
    if res.status() == StatusCode::METHOD_NOT_ALLOWED {
        res.headers_mut().insert(ALLOW, ALLOWED_METHODS);
    }
    Ok(res)
}

async fn set_cache_control(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let is_content_hashed = CONTENT_HASH.is_match(req.path());
    let mut res = next.call(req).await?;
    let status = res.status();
    if !res.headers().contains_key(CACHE_CONTROL) {
        let value =
            if is_content_hashed && (status.is_success() || status == StatusCode::NOT_MODIFIED) {
                IMMUTABLE
            } else {
                NO_CACHE
            };
        res.headers_mut().insert(CACHE_CONTROL, value);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::wasm("/universal-inbox-web-0123456789abcdef_bg.wasm", true)]
    #[case::js("/universal-inbox-web-0123456789abcdef.js", true)]
    #[case::snippet(
        "/snippets/universal-inbox-web-0123456789abcdef/public/js/index.js",
        true
    )]
    #[case::index("/index.html", false)]
    #[case::css("/css/universal-inbox.min.css", false)]
    #[case::short_hash("/app-0123456789abcde.js", false)]
    #[case::long_hash("/app-0123456789abcdef0.js", false)]
    fn test_content_hash_detection(#[case] path: &str, #[case] expected: bool) {
        assert_eq!(CONTENT_HASH.is_match(path), expected);
    }
}
