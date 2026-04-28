use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

pub fn static_routes() -> Router {
    let dist_dir = "admin-ui/dist";
    let index = format!("{dist_dir}/index.html");

    Router::new().fallback_service(
        ServeDir::new(dist_dir)
            .append_index_html_on_directories(true)
            .fallback(ServeFile::new(index)),
    )
}
