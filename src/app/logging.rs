use tracing_subscriber::{
    EnvFilter,
    fmt::layer,
    layer::{SubscriberExt},
    util::SubscriberInitExt,
};

pub fn init() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            layer()
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_target(false),
        )
        .init();
}
