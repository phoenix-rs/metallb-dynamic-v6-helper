mod k8s;

use std::fmt::Display;

use async_trait::async_trait;
pub use k8s::KubeClient;

use ipnet::Ipv6Net;
#[cfg(test)]
use mockall::automock;
use thiserror::Error;

#[derive(Error, Debug)]
pub struct ConnectorError {
    msg: String,
}
impl Display for ConnectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait Connector {
    async fn v6_ranges(&self) -> Result<Vec<Ipv6Net>, ConnectorError>;
    async fn replace(&self, old: &Ipv6Net, new: &Ipv6Net) -> Result<(), ConnectorError>;
    async fn insert(&self, range: &Ipv6Net) -> Result<(), ConnectorError>;
}
