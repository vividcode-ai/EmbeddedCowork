use crate::api_types::{AddressScope, NetworkAddress, NetworkFamily};

#[derive(Debug)]
pub struct ResolvedRemoteAddresses {
    pub all: Vec<NetworkAddress>,
    pub user_visible: Vec<NetworkAddress>,
    pub primary_remote_url: Option<String>,
}

pub fn resolve_network_addresses(host: &str, protocol: &str, port: u16) -> Vec<NetworkAddress> {
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    let mut add_address = |ip: &str, scope: AddressScope| {
        if ip.is_empty() || ip == "0.0.0.0" {
            return;
        }
        let key = format!("ipv4-{}", ip);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        results.push(NetworkAddress {
            ip: ip.to_string(),
            addr_family: NetworkFamily::Ipv4,
            scope,
            remote_url: format!("{}://{}:{}", protocol, ip, port),
        });
    };

    if host == "0.0.0.0" {
        if let Ok(interfaces) = std::env::var("EMBEDDEDCOWORK_TEST_INTERFACES") {
            for line in interfaces.lines() {
                let addr = line.trim();
                if !addr.is_empty() && addr != "0.0.0.0" {
                    add_address(addr, AddressScope::External);
                }
            }
        }
    }

    // Always include loopback
    add_address("127.0.0.1", AddressScope::Loopback);

    // Include configured host if it's a valid IPv4
    if is_ipv4_address(host) && host != "0.0.0.0" {
        let is_loopback = host.starts_with("127.");
        add_address(host, if is_loopback { AddressScope::Loopback } else { AddressScope::External });
    }

    results.sort_by(|a, b| {
        let scope_order = |s: &AddressScope| -> u8 {
            match s {
                AddressScope::External => 0,
                AddressScope::Internal => 1,
                AddressScope::Loopback => 2,
            }
        };
        scope_order(&a.scope).cmp(&scope_order(&b.scope))
    });

    results
}

pub fn resolve_remote_addresses(host: &str, protocol: &str, port: u16) -> ResolvedRemoteAddresses {
    let all = resolve_network_addresses(host, protocol, port);
    let user_visible = sort_user_visible_addresses(
        all.iter()
            .filter(|a| a.scope == AddressScope::External)
            .cloned()
            .collect(),
    );

    let primary_remote_url = user_visible.first().map(|a| a.remote_url.clone());

    ResolvedRemoteAddresses {
        all,
        user_visible,
        primary_remote_url,
    }
}

fn sort_user_visible_addresses(mut addresses: Vec<NetworkAddress>) -> Vec<NetworkAddress> {
    addresses.sort_by(|a, b| {
        get_user_visible_priority(&a.ip).cmp(&get_user_visible_priority(&b.ip))
    });
    addresses
}

fn get_user_visible_priority(ip: &str) -> u8 {
    if is_private_ipv4(ip) {
        return 0;
    }
    if is_link_local_ipv4(ip) {
        return 2;
    }
    1
}

fn is_link_local_ipv4(ip: &str) -> bool {
    if let Some(octets) = parse_ipv4(ip) {
        return octets[0] == 169 && octets[1] == 254;
    }
    false
}

fn is_private_ipv4(ip: &str) -> bool {
    if let Some(octets) = parse_ipv4(ip) {
        if octets[0] == 10 {
            return true;
        }
        if octets[0] == 192 && octets[1] == 168 {
            return true;
        }
        if octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31 {
            return true;
        }
    }
    false
}

fn parse_ipv4(value: &str) -> Option<Vec<u8>> {
    if !is_ipv4_address(value) {
        return None;
    }
    Some(value.split('.').map(|p| p.parse::<u8>().unwrap()).collect())
}

fn is_ipv4_address(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|part| {
        if part.is_empty() || part.len() > 3 {
            return false;
        }
        part.chars().all(|c| c.is_ascii_digit()) && {
            let num: u16 = part.parse().unwrap_or(256);
            num <= 255
        }
    })
}
