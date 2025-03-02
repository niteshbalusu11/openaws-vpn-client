use crate::config::Config;
use crate::log::Log;
use domain::base::iana::Class;
use domain::base::{Dname, Rtype};
use domain::rdata::A;
use rand::prelude::*;
use std::net::IpAddr;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;

pub struct DnsResolver {
    pub config: Arc<Config>,
    pub log: Arc<Log>,
    pub runtime: Arc<Runtime>,
}

impl DnsResolver {
    pub fn new(config: Arc<Config>, log: Arc<Log>, runtime: Arc<Runtime>) -> Self {
        Self {
            config,
            log,
            runtime,
        }
    }

    fn resolve_to_ip_list(&self, remote: String) -> Vec<IpAddr> {
        println!("Attempting DNS resolution for: {}", remote);
        self.log
            .append(format!("Looking up into '{}'...", remote).as_str());

        let resolver = domain::resolv::StubResolver::new();

        let d = match Dname::<Vec<u8>>::from_str(&remote) {
            Ok(d) => d,
            Err(e) => {
                println!("ERROR parsing domain name: {:?}", e);
                return vec![];
            }
        };

        println!("Sending DNS query...");
        let r = match self
            .runtime
            .block_on(async { resolver.query((d, Rtype::A, Class::In)).await })
        {
            Ok(r) => r,
            Err(e) => {
                println!("ERROR in DNS query: {:?}", e);
                return vec![];
            }
        };

        let msg = r.into_message();
        let ans = match msg.answer() {
            Ok(ans) => ans.limit_to::<A>(),
            Err(e) => {
                println!("ERROR getting answer section: {:?}", e);
                return vec![];
            }
        };

        let all = ans
            .filter(|v| v.is_ok())
            .map(|v| v.unwrap())
            .map(|v| v.into_data())
            .map(|v| v.addr())
            .map(|v| IpAddr::V4(v))
            .inspect(|v| self.log.append(format!("Resolved '{}'.", v).as_str()))
            .collect::<Vec<_>>();

        println!("Resolution returned {} IPs", all.len());
        all
    }

    pub fn resolve_addresses(&self) {
        println!("Resolving addresses...");
        println!("Attempting to lock remote mutex...");
        let remote_lock = self.config.remote.lock();
        println!("Got lock result: {:?}", remote_lock);
        let remote = remote_lock.unwrap().deref().clone().unwrap();
        println!("Released lock");

        let random_start = rng_domain();

        let remote_with_rng_domain = format!("{}.{}", random_start, remote.0);
        println!("Attempting to resolve: {}", remote_with_rng_domain);

        let mut all = self.resolve_to_ip_list(remote_with_rng_domain.clone());
        if all.is_empty() {
            self.log.append(format!(
                "Unable to resolve any addresses at '{}'.",
                remote_with_rng_domain.as_str()
            ));
            self.log
                .append("Attempting to resolve without any randomized domain...");
            all = self.resolve_to_ip_list(remote.0);
        };

        let mut br = self.config.addresses.lock().unwrap();
        *br = Some(all);
    }
}

fn rng_domain() -> String {
    let mut rng = thread_rng();
    let mut bts = [0u8; 12];
    rng.fill_bytes(&mut bts);
    hex::encode(bts)
}
