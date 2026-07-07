pub mod factories;
pub mod manager;
pub mod remote_pty;
pub mod renderable;
pub mod splash;
pub mod tab;
pub mod title;

#[cfg(test)]
pub use crate::layout::ContextDimension;

#[cfg(test)]
pub use factories::create_mock_context;
pub use factories::{next_rich_text_id, process_open_url};
pub use manager::{ContextManager, ContextManagerConfig};
pub use splash::SplashInjection;
pub use tab::Context;
