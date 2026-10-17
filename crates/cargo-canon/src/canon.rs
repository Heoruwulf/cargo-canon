//! Standalone `canon` CLI entrypoint.

#![forbid(unsafe_code)]

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cargo_canon::run().await
}
