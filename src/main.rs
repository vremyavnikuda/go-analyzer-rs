mod analysis;
mod backend;
mod types;
mod util;

use backend::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    eprintln!("Starting Go Analyzer LSP server...");

    // На Windows добавляем обработку сигналов для корректного завершения
    #[cfg(target_os = "windows")]
    {
        tokio::spawn(async {
            tokio::signal::ctrl_c().await.ok();
            eprintln!("Received shutdown signal, terminating Go Analyzer server...");
            std::process::exit(0);
        });
    }

    // На Unix системах обрабатываем SIGTERM и SIGINT
    #[cfg(not(target_os = "windows"))]
    {
        tokio::spawn(async {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            let mut sigint =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();

            tokio::select! {
                _ = sigterm.recv() => eprintln!("Received SIGTERM, terminating Go Analyzer server..."),
                _ = sigint.recv() => eprintln!("Received SIGINT, terminating Go Analyzer server..."),
            }
            std::process::exit(0);
        });
    }

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(Backend::new);

    eprintln!("Go Analyzer LSP server ready for connections");
    Server::new(stdin, stdout, socket).serve(service).await;
    eprintln!("Go Analyzer LSP server shutdown complete");
}
