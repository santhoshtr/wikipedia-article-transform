use actix_web::http::header::CONTENT_TYPE;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use serde::Deserialize;
use std::env;
use wikipedia_article_transform::{ArticleFormat, get_text};

#[derive(Deserialize)]
struct ArticlePath {
    language: String,
    title: String,
    format: String,
}

#[get("/healthz")]
async fn healthz() -> impl Responder {
    HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
        .body("ok")
}

async fn get_article(path: web::Path<ArticlePath>) -> impl Responder {
    let language = path.language.trim().to_lowercase();
    if language.is_empty()
        || !language
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return HttpResponse::BadRequest()
            .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
            .body("Invalid language code");
    }

    let title = path.title.trim();
    if title.is_empty() {
        return HttpResponse::BadRequest()
            .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
            .body("Article title is required");
    }

    let normalized_title = title.replace(' ', "_");
    let items = match get_text(&language, &normalized_title).await {
        Ok(items) => items,
        Err(err) => {
            let message = err.to_string();
            if message.contains("HTTP 404") {
                return HttpResponse::NotFound()
                    .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
                    .body("Article not found");
            }
            return HttpResponse::BadGateway()
                .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
                .body(format!("Upstream fetch failed: {message}"));
        }
    };

    match path.format.as_str() {
        "md" => HttpResponse::Ok()
            .insert_header((CONTENT_TYPE, "text/markdown; charset=utf-8"))
            .insert_header(("Cache-Control", "public, max-age=300"))
            .body(items.format_markdown()),
        "txt" => HttpResponse::Ok()
            .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
            .insert_header(("Cache-Control", "public, max-age=300"))
            .body(items.format_plain()),
        "json" => match items.format_json() {
            Ok(json) => HttpResponse::Ok()
                .insert_header((CONTENT_TYPE, "application/json"))
                .insert_header(("Cache-Control", "public, max-age=300"))
                .body(json),
            Err(err) => HttpResponse::InternalServerError()
                .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
                .body(format!("Failed to serialize JSON: {err}")),
        },
        _ => HttpResponse::NotFound()
            .insert_header((CONTENT_TYPE, "text/plain; charset=utf-8"))
            .body("Unsupported format. Use .md, .txt, or .json"),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(10000);

    HttpServer::new(|| {
        App::new()
            .service(healthz)
            .route("/{language}/{title}.{format}", web::get().to(get_article))
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
