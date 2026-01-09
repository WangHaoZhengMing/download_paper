pub mod connection;
pub mod headless;
pub mod pool;

pub use connection::connect_to_browser_and_page;
pub use pool::BrowserPool;

