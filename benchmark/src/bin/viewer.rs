use actix_files::Files;
use actix_web::{web, App, HttpResponse, HttpServer};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

struct AppState {
    results_dir: PathBuf,
}

async fn list_results(data: web::Data<Arc<AppState>>) -> HttpResponse {
    eprintln!("📥 API Request: /api/results");
    eprintln!("📂 Reading directory: {}", data.results_dir.display());

    match fs::read_dir(&data.results_dir) {
        Ok(entries) => {
            let files: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    eprintln!("  Found file: {}", path.display());
                    if path.extension()? == "json" {
                        let name = path.file_name()?.to_str().map(|s| s.to_string());
                        eprintln!("  ✅ JSON file: {:?}", name);
                        name
                    } else {
                        eprintln!("  ⏭️  Skipping non-JSON file");
                        None
                    }
                })
                .collect();

            eprintln!("📋 Total JSON files found: {}", files.len());

            match serde_json::to_string(&files) {
                Ok(json_str) => {
                    eprintln!("📤 Sending response: {}", json_str);
                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(json_str)
                }
                Err(e) => {
                    eprintln!("❌ JSON serialization error: {}", e);
                    HttpResponse::InternalServerError().body(format!("JSON error: {}", e))
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Error reading directory: {}", e);
            HttpResponse::InternalServerError().body(format!("Failed to read directory: {}", e))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let view_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("view");
    let results_dir = view_dir.join("results");

    if !view_dir.exists() {
        eprintln!(
            "❌ Error: View directory does not exist: {}",
            view_dir.display()
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("View directory not found: {}", view_dir.display()),
        ));
    }

    println!("🚀 KalamDB Benchmark Viewer");
    println!("📊 Starting server at http://localhost:3030");
    println!("🌐 Open your browser and navigate to http://localhost:3030");
    println!("📁 Serving files from: {}", view_dir.display());
    println!("📂 Results directory: {}", results_dir.display());
    println!();
    println!("Press Ctrl+C to stop");
    println!();

    let app_state = Arc::new(AppState {
        results_dir: results_dir.clone(),
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .route(
                "/",
                web::get().to(|| async {
                    HttpResponse::PermanentRedirect()
                        .insert_header(("Location", "/index.html"))
                        .finish()
                }),
            )
            .route("/api/results", web::get().to(list_results))
            .service(Files::new("/", view_dir.clone()).index_file("index.html"))
    })
    .bind(("127.0.0.1", 3030))?;

    println!("✅ Server bound to 127.0.0.1:3030");

    server.run().await
}
