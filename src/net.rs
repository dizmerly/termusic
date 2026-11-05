//! Connectivity check.
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// True if a TCP connection to a public DNS server succeeds within ~2s.
pub fn is_online() -> bool {
    let addr = SocketAddr::from(([8, 8, 8, 8], 53));
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok()
}
