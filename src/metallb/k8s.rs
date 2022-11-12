use ipnet::Ipv6Net;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use thiserror::Error;

#[cfg(test)]
use mockall::automock;

use super::{Connector, ConnectorError};

#[derive(Error, Debug)]
enum K8sError {
    #[error("Could not connect to k8s API: `{0}`")]
    ConnectionError(String),
    #[error("Could not find MetalLB AddressPool with name `{0}`")]
    PoolNotFound(String),
}

pub struct KubeClient;

#[cfg_attr(test, automock)]
impl KubeClient {
    /// Connects to the k8s API and looks for a MetalLB IpAddressPool with the given name in the default namespace
    /// An error is returned if no pool is found.
    pub fn new(name: &str) -> Result<Box<dyn Connector>, ConnectorError> {
        todo!()
    }
}

impl Connector for KubeClient {
    fn v6_ranges(&self) -> Result<Vec<Ipv6Net>, ConnectorError> {
        todo!()
    }

    fn replace(&self, old: &Ipv6Net, new: &Ipv6Net) -> Result<(), ConnectorError> {
        todo!()
    }

    fn insert(&self, range: &Ipv6Net) -> Result<(), ConnectorError> {
        todo!()
    }
}
