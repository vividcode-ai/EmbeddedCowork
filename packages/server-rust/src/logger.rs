use std::fmt;
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt::{format::Writer, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

#[derive(Clone)]
pub struct Logger {
    pub component: String,
}

impl Logger {
    pub fn new(component: &str) -> Self {
        Self {
            component: component.to_string(),
        }
    }

    pub fn child(&self, child_component: &str) -> Self {
        Self {
            component: format!("{}.{}", self.component, child_component),
        }
    }
}

#[allow(dead_code)]
struct ComponentLayer;

#[allow(dead_code)]
fn format_component(component: &str) -> String {
    let parts: Vec<&str> = component.split('.').collect();
    parts.join(":")
}

struct SimpleTimer;

impl FormatTime for SimpleTimer {
    fn format_time(&self, _w: &mut Writer<'_>) -> fmt::Result {
        // No timestamp output for simple console format
        Ok(())
    }
}

pub fn init_logger(level: Option<&str>, _component: &str) {
    init_logger_with_destination(level, _component, None)
}

pub fn init_logger_with_destination(level: Option<&str>, _component: &str, log_destination: Option<&str>) {
    let log_level = level
        .and_then(|l| {
            let l = l.to_lowercase();
            match l.as_str() {
                "trace" => Some(LevelFilter::TRACE),
                "debug" => Some(LevelFilter::DEBUG),
                "info" => Some(LevelFilter::INFO),
                "warn" => Some(LevelFilter::WARN),
                "error" => Some(LevelFilter::ERROR),
                _ => None,
            }
        })
        .unwrap_or(LevelFilter::INFO);

    let env_filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy();

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_timer(SimpleTimer)
        .with_level(true);

    if let Some(dest) = log_destination {
        let file = std::fs::File::create(dest).expect("Failed to create log file");
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.with_writer(file))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.with_writer(std::io::stdout))
            .init();
    }
}

// Helper macros
#[macro_export]
macro_rules! log_trace {
    ($logger:expr, $msg:expr) => {
        tracing::trace!(component = %$logger.component, message = %$msg)
    };
    ($logger:expr, $msg:expr, $($key:ident = $value:expr),*) => {
        tracing::trace!(component = %$logger.component, message = %$msg, $($key = $value),*)
    };
}

#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $msg:expr) => {
        tracing::debug!(component = %$logger.component, message = %$msg)
    };
    ($logger:expr, $msg:expr, $($key:ident = $value:expr),*) => {
        tracing::debug!(component = %$logger.component, message = %$msg, $($key = $value),*)
    };
}

#[macro_export]
macro_rules! log_info {
    ($logger:expr, $msg:expr) => {
        tracing::info!(component = %$logger.component, message = %$msg)
    };
    ($logger:expr, $msg:expr, $($key:ident = $value:expr),*) => {
        tracing::info!(component = %$logger.component, message = %$msg, $($key = $value),*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($logger:expr, $msg:expr) => {
        tracing::warn!(component = %$logger.component, message = %$msg)
    };
    ($logger:expr, $msg:expr, $($key:ident = $value:expr),*) => {
        tracing::warn!(component = %$logger.component, message = %$msg, $($key = $value),*)
    };
}

#[macro_export]
macro_rules! log_error {
    ($logger:expr, $msg:expr) => {
        tracing::error!(component = %$logger.component, message = %$msg)
    };
    ($logger:expr, $msg:expr, $($key:ident = $value:expr),*) => {
        tracing::error!(component = %$logger.component, message = %$msg, $($key = $value),*)
    };
}
