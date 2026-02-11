mod analysis;
mod backend;
mod semantic;
mod types;
mod util;

use backend::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    eprintln!("Starting Go Analyzer LSP server...");
    #[cfg(target_os = "windows")]
    {
        tokio::spawn(async {
            tokio::signal::ctrl_c().await.ok();
            eprintln!("Received shutdown signal, terminating Go Analyzer server...");
            std::process::exit(0);
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        tokio::spawn(async {
            let sigterm_result =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
            let sigint_result =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt());
            match (sigterm_result, sigint_result) {
                (Ok(mut sigterm), Ok(mut sigint)) => {
                    tokio::select! {
                        _ = sigterm.recv() => eprintln!("Received SIGTERM, terminating Go Analyzer server..."),
                        _ = sigint.recv() => eprintln!("Received SIGINT, terminating Go Analyzer server..."),
                    }
                    std::process::exit(0);
                }
                _ => {
                    eprintln!(
                        "Failed to setup signal handlers, continuing without signal handling"
                    );
                }
            }
        });
    }
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(Backend::new);
    eprintln!("Go Analyzer LSP server ready for connections");
    Server::new(stdin, stdout, socket).serve(service).await;
    eprintln!("Go Analyzer LSP server shutdown complete");
}
