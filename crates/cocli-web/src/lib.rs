//! Static asset and SPA fallback serving for the local cocli workspace.

use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

#[cfg(feature = "embed-web")]
use rust_embed::RustEmbed;

#[cfg(feature = "embed-web")]
#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct EmbeddedAssets;

#[derive(Clone, Debug)]
struct WebState {
    root: Option<PathBuf>,
}

/// Builds a static asset router with an SPA fallback to `index.html`.
///
/// The filesystem root is used for local development. Release builds can
/// enable `embed-web` to serve the Vite output directly from the binary.
pub fn router(root: Option<PathBuf>) -> Router {
    Router::new()
        .fallback(get(serve_asset))
        .with_state(WebState { root })
}

async fn serve_asset(State(state): State<WebState>, OriginalUri(uri): OriginalUri) -> Response {
    let Some(asset_path) = safe_asset_path(uri.path()) else {
        return response(
            StatusCode::BAD_REQUEST,
            "text/plain; charset=utf-8",
            "invalid asset path".as_bytes().to_vec(),
        );
    };

    if let Some((bytes, content_type)) = load_asset(&state, &asset_path).await {
        return response(StatusCode::OK, &content_type, bytes);
    }

    response(
        StatusCode::NOT_FOUND,
        "text/plain; charset=utf-8",
        b"cocli web assets are not built; run `npm --prefix web run build` or pass --web-dir"
            .to_vec(),
    )
}

fn safe_asset_path(request_path: &str) -> Option<PathBuf> {
    let relative = request_path.trim_start_matches('/');
    let path = if relative.is_empty() {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(relative)
    };
    if path
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    {
        Some(path)
    } else {
        None
    }
}

async fn load_asset(state: &WebState, requested: &Path) -> Option<(Vec<u8>, String)> {
    if let Some(root) = &state.root {
        if let Some(asset) = load_filesystem_asset(root, requested).await {
            return Some(asset);
        }
    }

    #[cfg(feature = "embed-web")]
    if let Some(asset) = load_embedded_asset(requested) {
        return Some(asset);
    }

    None
}

async fn load_filesystem_asset(root: &Path, requested: &Path) -> Option<(Vec<u8>, String)> {
    let requested_path = root.join(requested);
    if requested_path.is_file() {
        return read_file(&requested_path, requested).await;
    }

    let index = Path::new("index.html");
    read_file(&root.join(index), index).await
}

async fn read_file(path: &Path, logical_path: &Path) -> Option<(Vec<u8>, String)> {
    let bytes = tokio::fs::read(path).await.ok()?;
    let content_type = mime_guess::from_path(logical_path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    Some((bytes, content_type))
}

#[cfg(feature = "embed-web")]
fn load_embedded_asset(requested: &Path) -> Option<(Vec<u8>, String)> {
    let requested = requested.to_str()?;
    let (asset, logical_path) = EmbeddedAssets::get(requested)
        .map(|asset| (asset, requested))
        .or_else(|| EmbeddedAssets::get("index.html").map(|asset| (asset, "index.html")))?;
    let content_type = mime_guess::from_path(logical_path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    Some((asset.data.into_owned(), content_type))
}

fn response(status: StatusCode, content_type: &str, bytes: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{header, HeaderValue, Request, StatusCode};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::router;

    #[tokio::test]
    async fn serves_assets_and_spa_fallback_from_disk() {
        let directory = tempdir().expect("temporary web root");
        tokio::fs::write(directory.path().join("index.html"), "<main>cocli</main>")
            .await
            .expect("write index");
        tokio::fs::write(directory.path().join("app.js"), "console.log('cocli')")
            .await
            .expect("write script");
        let app = router(Some(directory.path().to_path_buf()));

        let script = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/app.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("script response");
        assert_eq!(script.status(), StatusCode::OK);
        assert_eq!(
            script.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/javascript"))
        );

        let fallback = app
            .oneshot(
                Request::builder()
                    .uri("/channel/local")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("fallback response");
        let body = to_bytes(fallback.into_body(), 1024)
            .await
            .expect("fallback body");
        assert_eq!(body, "<main>cocli</main>");
    }
}
