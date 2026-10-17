//! Structured logging initialization via tracing and tracing-subscriber.

use crate::error::CoreError;
use tracing_subscriber::EnvFilter;

/// Initializes tracing subscriber with environment filter support and optional JSON framing.
pub fn init_logging(verbosity: u8, json: bool) -> Result<(), CoreError> {
    let default_directive = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_directive));

    let subscriber_builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true);

    if json {
        subscriber_builder
            .json()
            .try_init()
            .map_err(|e| CoreError::LoggingInit(e.to_string()))?;
    } else {
        subscriber_builder
            .try_init()
            .map_err(|e| CoreError::LoggingInit(e.to_string()))?;
    }

    Ok(())
}
