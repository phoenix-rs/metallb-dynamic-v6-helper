use std::str::FromStr;

use async_trait::async_trait;
use ipnet::Ipv6Net;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Patch, PatchParams},
    core::ObjectMeta,
    Api, Client, CustomResource,
};
use log::{debug, info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Connector, ConnectorError};

const METALLB_IPADDRPOOL_CRD_NAME: &str = "ipaddresspools.metallb.io";

#[derive(Error, Debug)]
enum K8sError {
    #[error("Error while accessing the k8s API: `{0}`")]
    ConnectionError(String),
    #[error("Could not find MetalLB AddressPool with name `{0}`")]
    PoolNotFound(String),
    #[error("MetalLB IPAddressPool CRD does not exist, please make sure that it is installed")]
    CRDNotFound,
    #[error("Could not replace range `{0}` with `{1}` as it does not exist")]
    RangeNotFound(String, String),
    #[error("Error while updating the ResourcePool: `{0}`")]
    PoolUpdateError(String),
}
impl From<K8sError> for ConnectorError {
    fn from(value: K8sError) -> Self {
        value.into()
    }
}

// Manual implementation of the AddressPool CRD spec.
// don't think there is a way to generate this at runtime based on the Cluster response
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema, Default)]
#[kube(
    group = "metallb.io",
    version = "v1beta1",
    kind = "IpAddressPool",
    namespaced
)]
#[allow(non_snake_case)]
struct IpAddressPoolSpec {
    addresses: Vec<String>,
    autoAssign: Option<bool>,
    avoidBuggyIPs: Option<bool>,
}

pub struct KubeClient<'a> {
    name: &'a str,
    client: Client,
}

impl KubeClient<'_> {
    /// Connects to the k8s API and looks for a MetalLB IpAddressPool with the given name in the default namespace
    /// An error is returned if no pool is found.
    pub async fn try_new(name: &str) -> Result<Box<dyn Connector + '_>, ConnectorError> {
        let c = match Client::try_default().await {
            Ok(c) => c,
            Err(e) => return Err(K8sError::ConnectionError(e.to_string()).into()),
        };

        let crds: Api<CustomResourceDefinition> = Api::all(c.clone());
        let p = match crds.get_opt(METALLB_IPADDRPOOL_CRD_NAME).await {
            Ok(p) => p,
            Err(e) => return Err(K8sError::ConnectionError(e.to_string()).into()),
        };

        if p.is_none() {
            return Err(K8sError::CRDNotFound.into());
        }

        let kclient = KubeClient { name, client: c };

        match kclient.find_pool().await {
            Ok(_) => {}
            Err(e) => {
                warn!("Error encountered when trying to read IPAddressPool: {}", e)
            }
        }
        Ok(Box::new(kclient))
    }

    async fn find_pool(&self) -> Result<IpAddressPool, K8sError> {
        let pools_api: Api<IpAddressPool> = Api::default_namespaced(self.client.clone());

        match pools_api.get_opt(self.name).await {
            Ok(p) => match p {
                Some(p) => Ok(p),
                None => Err(K8sError::PoolNotFound(self.name.to_string())),
            },
            Err(e) => Err(K8sError::ConnectionError(e.to_string())),
        }
    }

    fn gen_patch(&self, pool: Vec<String>) -> Patch<IpAddressPool> {
        let p = Patch::Apply(IpAddressPool {
            metadata: ObjectMeta {
                name: Some(self.name.into()),
                ..ObjectMeta::default()
            },
            spec: IpAddressPoolSpec {
                addresses: pool,
                ..IpAddressPoolSpec::default()
            },
        });
        debug!("Generated Patch: {:?}", p);
        p
    }
}

#[async_trait]
impl Connector for KubeClient<'_> {
    async fn v6_ranges(&self) -> Result<Vec<Ipv6Net>, ConnectorError> {
        let mut ranges = Vec::new();
        let r = self.find_pool().await?;

        for range_str in &r.spec.addresses {
            match Ipv6Net::from_str(range_str) {
                Ok(r) => ranges.push(r),
                Err(e) => {
                    debug!("Not a V6 range, skipping: {}, {}", range_str, e);
                    continue;
                }
            };
        }
        debug!("Found IPv6 range in pool {}: {:?}", self.name, ranges);
        Ok(ranges)
    }

    async fn replace(&self, old: &Ipv6Net, new: &Ipv6Net) -> Result<(), ConnectorError> {
        let pools_api: Api<IpAddressPool> = Api::default_namespaced(self.client.clone());
        let pool = self.find_pool().await?;

        // This vec contains all addresses *except* for the old address, we can then add our new range if it makes sense
        let mut patched_addrs: Vec<String> = pool
            .spec
            .addresses
            .iter()
            .cloned()
            .filter(|addr| addr != &old.to_string())
            .collect();
        match (
            net_in_pool(&pool, old).is_some(),
            net_in_pool(&pool, new).is_some(),
        ) {
            (false, false) => {
                // Neither the old or new address exist, we can't replace anything
                return Err(K8sError::RangeNotFound(old.to_string(), new.to_string()).into());
            }
            (false, true) => {
                info!(
                    "New range {} already exists and old range {} is absent, doing nothing",
                    new, old
                );
                return Ok(());
            }
            (true, true) => {
                info!("New and old range both exist, deleting old range {}", old);
            }
            (true, false) => {
                // Normal case, insert our new address
                patched_addrs.push(new.to_string());
            }
        };

        match pools_api
            .patch(
                self.name,
                &PatchParams::default(),
                &self.gen_patch(patched_addrs),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(K8sError::PoolUpdateError(e.to_string()).into()),
        }
    }

    async fn insert(&self, range: &Ipv6Net) -> Result<(), ConnectorError> {
        let pools_api: Api<IpAddressPool> = Api::default_namespaced(self.client.clone());
        let mut pool = self.find_pool().await?;

        let None = net_in_pool(&pool, range) else {
            info!("Range {} already in pool, not inserting", range);
            return Ok(());
        };

        pool.spec.addresses.push(range.to_string());
        match pools_api
            .patch(
                self.name,
                &PatchParams::default(),
                &self.gen_patch(pool.spec.addresses),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(K8sError::PoolUpdateError(e.to_string()).into()),
        }
    }
}

// Checks whether the address exists in the IPAddressPool, returns the index as an option if found
fn net_in_pool(pool: &IpAddressPool, addr: &Ipv6Net) -> Option<usize> {
    let mut pos = None;
    for (i, a) in pool.spec.addresses.iter().enumerate() {
        if a == &addr.to_string() {
            pos = Some(i);
        }
    }
    pos
}
