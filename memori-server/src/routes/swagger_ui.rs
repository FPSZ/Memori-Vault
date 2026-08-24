//! Swagger UI 文档页：把 `/api/openapi.json` 渲染成可交互的 API 文档界面。
//!
//! - spec 仍由表驱动 `openapi.rs` 生成，本模块只负责 UI 展示层；
//! - 静态资源（HTML/CSS/JS）构建期 `include_*!` 编译进二进制，**离线可用**，
//!   符合本地优先原则；不引入 utoipa 等额外依赖。

use axum::http::header;
use axum::response::Response;

const SWAGGER_UI_HTML: &str = include_str!("../../assets/swagger_ui.html");
const SWAGGER_UI_CSS: &[u8] = include_bytes!("../../assets/swagger-ui.css");
const SWAGGER_UI_JS: &[u8] = include_bytes!("../../assets/swagger-ui-bundle.js");
const SWAGGER_UI_STANDALONE_JS: &[u8] =
    include_bytes!("../../assets/swagger-ui-standalone-preset.js");

/// `GET /api/docs`：Swagger UI 页面（数据源指向 `/api/openapi.json`）。
/// 入口页显式 no-cache：模板随二进制更新，避免浏览器缓存旧页面。
pub(crate) async fn docs_page() -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(axum::body::Body::from(SWAGGER_UI_HTML))
        .expect("static html response")
}

/// `GET /api/docs/swagger-ui.css`：UI 样式（构建期内置）。
pub(crate) async fn docs_css() -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(axum::body::Body::from(SWAGGER_UI_CSS))
        .expect("static css response")
}

/// `GET /api/docs/swagger-ui-bundle.js`：UI 逻辑（构建期内置）。
pub(crate) async fn docs_js() -> Response {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(axum::body::Body::from(SWAGGER_UI_JS))
        .expect("static js response")
}

/// `GET /api/docs/swagger-ui-standalone-preset.js`：StandaloneLayout 布局插件
/// （`swagger-ui-bundle.js` 不含该布局，缺失会报 "No layout defined for StandaloneLayout"）。
pub(crate) async fn docs_standalone_js() -> Response {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(axum::body::Body::from(SWAGGER_UI_STANDALONE_JS))
        .expect("static standalone js response")
}
