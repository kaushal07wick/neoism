pub mod assistant;
pub mod dialog;
pub mod welcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePath {
    Terminal,
    Welcome,
    ConfirmQuit,
}
