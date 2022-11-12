mod k8s;

pub use k8s::KubeClient;

use ipnet::Ipv6Net;
#[cfg(test)]
use mockall::automock;
use thiserror::Error;

pub type ConnectorError = String;

#[cfg_attr(test, automock)]
pub trait Connector {
    fn v6_ranges(&self) -> Result<Vec<Ipv6Net>, ConnectorError>;
    fn replace(&self, old: &Ipv6Net, new: &Ipv6Net) -> Result<(), ConnectorError>;
    fn insert(&self, range: &Ipv6Net) -> Result<(), ConnectorError>;
}
