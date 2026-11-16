//! Serving the built admin console.
//!
//! The console is a SvelteKit static bundle dropped in at `./frontend/build`.
//! Every path that is not a gateway route and not a vendor request is looked up
//! here, and anything that does not exist falls back to the SPA entry point so
//! client-side routing keeps working.

use axum::body::Body;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// Directory holding the built frontend, relative to the process working
/// directory.
pub const FRONTEND_DIR: &str = "./frontend/build";

/// Serves `path` from the built frontend, falling back to the SPA entry point.
pub async fn serve_static(path: &str) -> axum::response::Response {
    serve_from(FRONTEND_DIR, path).await
}

/// Serves `path` out of `base_dir`, falling back to `<base_dir>/index.html`.
///
/// `ServeDir` percent-decodes the request path and then rejects any `..` or
/// absolute component outright, so a crafted path can only ever address a file
/// inside `base_dir`. Joining the raw path onto the base directory instead — as
/// this code used to — let `..` walk straight out of it and read any file the
/// process could open, including `/proc/self/environ`.
///
/// Assets come back with correct MIME types and conditional-request support,
/// neither of which the hand-rolled reader had.
///
/// The base directory is a parameter so the containment behaviour can be tested
/// against a directory that is known to exist.
pub async fn serve_from(base_dir: &str, path: &str) -> axum::response::Response {
    let service =
        ServeDir::new(base_dir).fallback(ServeFile::new(format!("{base_dir}/index.html")));

    let Ok(request) = axum::http::Request::builder()
        .uri(format!("/{path}"))
        .body(Body::empty())
    else {
        return not_found();
    };

    // ServeDir never fails to answer: its error type is `Infallible`.
    match service.oneshot(request).await {
        Ok(response) => {
            // ServeDir streams files through its own body type; rewrap it so the
            // router's handler signature (which is `axum::body::Body`) holds.
            let (parts, body) = response.into_parts();
            axum::response::Response::from_parts(parts, Body::new(body))
        }
        Err(unreachable) => match unreachable {},
    }
}

fn not_found() -> axum::response::Response {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A secret that lives *outside* the directory being served, so any read of
    /// it proves the request escaped containment.
    const OUTSIDE_SECRET: &str = "sluice-containment-canary-7f3a";

    /// Builds a directory with one asset and one SPA entry point, plus a
    /// canary file one level above it.
    async fn fixture() -> (PathBuf, String) {
        // A unique directory under the system temp dir, so the fixture never
        // collides with a real build output.
        let unique = format!(
            "sluice-static-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let base = root.join("build");
        std::fs::create_dir_all(base.join("_app")).unwrap();
        std::fs::write(base.join("index.html"), b"<html>spa</html>").unwrap();
        std::fs::write(base.join("_app").join("app.js"), b"console.log(1)").unwrap();
        // The canary sits beside `build`, not inside it.
        std::fs::write(root.join("canary.txt"), OUTSIDE_SECRET).unwrap();

        (root, base.to_string_lossy().into_owned())
    }

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// A request path may only ever address a file inside the served directory.
    /// These are the shapes an attacker sends to try to break out of it.
    #[tokio::test]
    async fn dot_dot_cannot_escape_the_served_directory() {
        let (_root, base) = fixture().await;

        for attempted in [
            "../canary.txt",
            "../../canary.txt",
            "../../../etc/passwd",
            "..%2fcanary.txt",
            "%2e%2e/canary.txt",
            "....//....//canary.txt",
            "_app/../../../canary.txt",
            "..\\canary.txt",
        ] {
            let response = serve_from(&base, attempted).await;
            let body = body_text(response).await;

            assert!(
                !body.contains(OUTSIDE_SECRET) && !body.contains("root:x:"),
                "path {attempted:?} escaped {base:?}"
            );
        }
    }

    /// A real asset inside the directory is still served, so the containment
    /// check has not simply broken static serving.
    #[tokio::test]
    async fn asset_inside_the_directory_is_served() {
        let (_root, base) = fixture().await;

        let response = serve_from(&base, "_app/app.js").await;
        assert_eq!(response.status(), 200);
        assert_eq!(body_text(response).await, "console.log(1)");
    }

    /// A missing asset falls back to the SPA entry point, which is what makes
    /// client-side routing work on a hard refresh of a deep link.
    #[tokio::test]
    async fn unknown_path_falls_back_to_the_spa_entry_point() {
        let (_root, base) = fixture().await;

        let response = serve_from(&base, "some/client/route/that/does/not/exist").await;
        assert_eq!(response.status(), 200);
        assert_eq!(body_text(response).await, "<html>spa</html>");
    }

    /// NUL bytes must not reach the filesystem as part of a path.
    #[tokio::test]
    async fn nul_byte_is_rejected() {
        let (_root, base) = fixture().await;

        let response = serve_from(&base, "index.html\0.txt").await;
        assert_ne!(response.status(), 500);
    }
}
