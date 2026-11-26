//! Local transports for the current desktop user.
#[cfg(unix)]
pub use tokio::net::UnixListener as Listener;
#[cfg(unix)]
pub async fn connect(path: &std::path::Path) -> std::io::Result<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(path).await
}
#[cfg(windows)]
mod pipe;
#[cfg(windows)]
pub use pipe::{
    Listener, account_user_sid, connect, connect_to_owner, create_private_directory,
    process_user_sid,
};
