mod iface;
pub use iface::IfaceSource;

use std::fmt::Display;

use ipnet::Ipv6Net;
#[cfg(test)]
use mockall::automock;
use thiserror::Error;

#[derive(Error, Debug)]
pub struct SourceError {
    msg: String,
}
impl Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[cfg_attr(test, automock)]
pub trait PrefixSource {
    fn v6_network(&self) -> Result<Ipv6Net, SourceError>;
}
