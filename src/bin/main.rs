mod config;

use std::time::Duration;
use std::{error::Error, net::Ipv6Addr};

use clap::Parser;
use env_logger::Builder;
use ipnet::{Ipv6Net, PrefixLenError};
use log::{debug, error, info};

use config::Config;

use metallb_v6_prefix_helper::{
    metallb::{Connector, KubeClient},
    prefix::{IfaceSource, PrefixSource},
    IPV6_NETMASK,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::parse();
    Builder::new().filter_level(config.loglevel.into()).init();
    debug!("Parsed config: {:?}", config);

    let source = match config.source {
        config::Source::Iface => IfaceSource::try_new(config.iface.clone())?,
    };
    debug!("Initialized source {:?}", config.source);
    let pool = KubeClient::try_new(config.metallb_address_pool.as_str()).await?;
    debug!("initialized MetalLB pool {:?}", config.metallb_address_pool);

    loop {
        match run(source.as_ref(), pool.as_ref(), &config).await {
            Ok(_) => {}
            Err(e) => error!("Error: {}", e),
        };
        sleep(Duration::from_secs(config.interval)).await;
    }
}

#[cfg(test)]
#[tokio::main]
async fn test_run(
    source: &dyn PrefixSource,
    pool_conn: &dyn Connector,
    config: &Config,
) -> Result<(), Box<dyn Error>> {
    run(source, pool_conn, config).await
}

async fn run(
    source: &dyn PrefixSource,
    pool_conn: &dyn Connector,
    config: &Config,
) -> Result<(), Box<dyn Error>> {
    let current_ranges = pool_conn.v6_ranges().await?;
    info!(
        "Found the following Ipv6 ranges in pool {}: {:?}",
        config.metallb_address_pool, current_ranges
    );
    let current_range = find_dynamic_mlb_range(&current_ranges, &config.metallb_host_range);
    let target_network = source.v6_network()?;
    info!("Determined desired IPv6 network to be {}", target_network);
    let target_range = generate_target_range(&target_network, &config.metallb_host_range)?;
    info!("Calculated desired MetalLB range: {}", target_range);

    match current_range {
        Some(current_range) => {
            if current_range == &target_range {
                info!(
                    "Target IPv6 range {} already present in MetalLB pool, nothing to do",
                    target_range
                );
                Ok(())
            } else {
                info!(
                    "Range in MetalLB pool ({}) outdated, replacing with new range: {}",
                    current_range, target_range
                );
                pool_conn.replace(current_range, &target_range).await?;
                Ok(())
            }
        }
        None => {
            info!(
                "No existing IPv6 range matches address pool {}, adding range {}",
                config.metallb_address_pool, target_range
            );
            pool_conn.insert(&target_range).await?;
            Ok(())
        }
    }
}

fn generate_target_range<'a>(
    dyn_net: &'a Ipv6Net,
    mlb_range: &'a Ipv6Net,
) -> Result<Ipv6Net, PrefixLenError> {
    let net_sanitized = u128::from(dyn_net.addr()) & IPV6_NETMASK;
    let range_sanitized = u128::from(mlb_range.addr()) & !IPV6_NETMASK;

    Ipv6Net::new(
        (net_sanitized | range_sanitized).into(),
        mlb_range.prefix_len(),
    )
}

fn find_dynamic_mlb_range<'a>(ranges: &'a [Ipv6Net], host_range: &Ipv6Net) -> Option<&'a Ipv6Net> {
    for r in ranges {
        let host_part = u128::from(r.addr()) & !IPV6_NETMASK;
        if Ipv6Addr::from(host_part) == host_range.addr() {
            return Some(r);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use async_trait::async_trait;
    use ipnet::Ipv6Net;
    use metallb_v6_prefix_helper::{
        metallb::{Connector, ConnectorError},
        prefix::{PrefixSource, SourceError},
    };
    use mockall::{mock, predicate};

    use crate::{config::Config, test_run};

    fn config() -> Config {
        Config {
            metallb_address_pool: "my-pool".to_string(),
            metallb_host_range: Ipv6Net::from_str("::abab:cdcd:0:0/80").unwrap(),
            source: crate::config::Source::Iface,
            iface: "eth0".to_string(),
            loglevel: crate::config::Loglevel::Info,
            interval: 60,
        }
    }

    const TARGET_NET: &str = "2001:db8:1111:1111::/64";
    fn range_other() -> Ipv6Net {
        Ipv6Net::from_str("fd42:aaaa::/64").unwrap()
    }
    fn range_outdated() -> Ipv6Net {
        Ipv6Net::from_str("2001:db8:0:0:abab:cdcd:0:0/80").unwrap()
    }
    fn range_correct() -> Ipv6Net {
        Ipv6Net::from_str("2001:db8:1111:1111:abab:cdcd:0:0/80").unwrap()
    }

    mock! {
        PrefixSource {}
        impl PrefixSource for PrefixSource {
            fn v6_network(&self) -> Result<Ipv6Net, SourceError>;
        }
    }
    mock! {
        Connector {}
        #[async_trait]
        impl Connector for Connector {
            async fn v6_ranges(&self) -> Result<Vec<Ipv6Net>, ConnectorError>;
            async fn replace(&self, old: &Ipv6Net, new: &Ipv6Net) -> Result<(), ConnectorError>;
            async fn insert(&self, range: &Ipv6Net) -> Result<(), ConnectorError>;
        }
    }

    fn mock_source() -> MockPrefixSource {
        let mut mock = MockPrefixSource::new();
        mock.expect_v6_network()
            .returning(|| Ok(Ipv6Net::from_str(TARGET_NET).unwrap()));
        mock
    }

    #[test]
    fn creates_missing_range() {
        let mock_source = mock_source();
        let mut mock_pool = MockConnector::new();
        mock_pool
            .expect_v6_ranges()
            .once()
            .returning(|| Ok(vec![range_other()]));
        mock_pool
            .expect_insert()
            .once()
            .with(predicate::eq(range_correct()))
            .returning(|_| Ok(()));

        test_run(
            Box::new(mock_source).as_ref(),
            Box::new(mock_pool).as_ref(),
            &config(),
        )
        .unwrap();
    }

    #[test]
    fn updates_outdated_range() {
        let mock_source = mock_source();
        let mut mock_pool = MockConnector::new();
        mock_pool
            .expect_v6_ranges()
            .once()
            .returning(|| Ok(vec![range_outdated(), range_other()]));
        mock_pool
            .expect_replace()
            .once()
            .with(
                predicate::eq(range_outdated()),
                predicate::eq(range_correct()),
            )
            .returning(|_, _| Ok(()));

        test_run(
            Box::new(mock_source).as_ref(),
            Box::new(mock_pool).as_ref(),
            &config(),
        )
        .unwrap();
    }

    #[test]
    fn detects_correct_range() {
        let mock_source = mock_source();
        let mut mock_pool = MockConnector::new();
        mock_pool
            .expect_v6_ranges()
            .once()
            .returning(|| Ok(vec![range_correct(), range_other()]));
        test_run(
            Box::new(mock_source).as_ref(),
            Box::new(mock_pool).as_ref(),
            &config(),
        )
        .unwrap();
    }
}
