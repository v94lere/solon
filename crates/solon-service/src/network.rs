//! Réseau sortant de la machine : réseau HNS (Host Network Service) de type ICS, comme WSL2.
//!
//! ICS fournit NAT, DHCP et proxy DNS sur la passerelle ; on n'utilise que le NAT et la passerelle :
//! l'adresse de l'invité est **statique**, fournie à l'agent (`ConfigureNetwork`) à partir des
//! propriétés de l'endpoint. Références : documentation HCN, dépôt `skorhone/wsl2-custom-network`
//! (JSON WSL : `Type=ICS`, `Flags=9`, `IsolateSwitch=true`).
//!
//! Point d'attention (ARCHITECTURE.md §6.2) : les VPN d'entreprise et les collisions de plages IP
//! sont les pannes connues de cette approche ; la plage est choisie parmi des candidates en évitant
//! celles déjà routées sur l'hôte.

use std::ffi::c_void;
use std::net::Ipv4Addr;

use solon_core::protocol::NetworkConfig;
use solon_core::{ErrorCode, Result, SolonError};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::System::HostComputeNetwork::{
    HcnCloseEndpoint, HcnCloseNetwork, HcnCreateEndpoint, HcnCreateNetwork, HcnDeleteEndpoint,
    HcnDeleteNetwork, HcnEnumerateNetworks, HcnOpenNetwork, HcnQueryEndpointProperties,
    HcnQueryNetworkProperties,
};
use windows::core::{GUID, HSTRING, PWSTR};

pub const NETWORK_NAME: &str = "Solon";
/// Identifiant fixe du réseau Solon : un seul réseau, retrouvé à chaque démarrage.
pub const NETWORK_ID: GUID = GUID::from_u128(0x5010_0000_0000_4000_8000_0000_0000_0001);

/// Plages candidates, essayées dans l'ordre en sautant celles déjà utilisées sur l'hôte.
pub const CANDIDATE_PREFIXES: &[&str] = &[
    "172.30.0",
    "172.30.1",
    "172.31.200",
    "192.168.190",
    "192.168.213",
    "10.250.250",
];

fn take_doc(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { p.to_string() }.ok();
    unsafe { LocalFree(Some(HLOCAL(p.0 as *mut c_void))) };
    s
}

fn hcn_err(context: &str, e: &windows::core::Error, record: Option<String>) -> SolonError {
    let hresult = e.code().0 as u32;
    let mut msg = format!("{context} : {} (0x{hresult:08X})", e.message());
    if let Some(r) = record.filter(|r| !r.trim().is_empty()) {
        msg.push_str(" — ");
        msg.push_str(&r);
    }
    SolonError::new(ErrorCode::HcsError, msg).with_hresult(hresult)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuestNetwork {
    pub network_id: String,
    pub endpoint_id: String,
    pub mac_address: String,
    pub address: Ipv4Addr,
    pub prefix_len: u8,
    pub gateway: Ipv4Addr,
    pub dns: Vec<String>,
}

impl GuestNetwork {
    pub fn to_config(&self) -> NetworkConfig {
        NetworkConfig {
            interface: "eth0".into(),
            address: self.address.to_string(),
            prefix_len: self.prefix_len,
            gateway: self.gateway.to_string(),
            dns: self.dns.clone(),
            mtu: None,
            search_domains: vec![],
        }
    }
}

/// Sous-réseaux /24 déjà présents sur l'hôte (réseaux HNS existants + adresses des cartes).
fn used_prefixes() -> Vec<String> {
    let mut used = Vec::new();
    if let Ok(list) = enumerate_networks() {
        for id in list {
            if let Ok(props) = network_properties(&id) {
                for s in extract_prefixes(&props) {
                    used.push(s);
                }
            }
        }
    }
    used.extend(host_adapter_prefixes());
    used
}

fn extract_prefixes(props: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![props];
    while let Some(v) = stack.pop() {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    if (k == "AddressPrefix" || k == "IpAddressPrefix") && v.is_string() {
                        if let Some(p) = v
                            .as_str()
                            .and_then(|s| s.split('/').next())
                            .and_then(|ip| ip.rsplit_once('.'))
                        {
                            out.push(p.0.to_owned());
                        }
                    } else {
                        stack.push(v);
                    }
                }
            }
            serde_json::Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    out
}

/// Préfixes /24 des adresses IPv4 des cartes de l'hôte (`GetAdaptersAddresses`).
fn host_adapter_prefixes() -> Vec<String> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u8> = vec![0; size as usize];
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    for _ in 0..3 {
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                flags,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if rc == 111 {
            buf.resize(size as usize, 0);
            continue;
        }
        if rc != 0 {
            return vec![];
        }
        break;
    }
    let mut out = Vec::new();
    let mut adapter = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !adapter.is_null() {
        let a = unsafe { &*adapter };
        let mut ua = a.FirstUnicastAddress;
        while !ua.is_null() {
            let u = unsafe { &*ua };
            if !u.Address.lpSockaddr.is_null() {
                let sa = unsafe { &*(u.Address.lpSockaddr as *const SOCKADDR_IN) };
                if sa.sin_family == AF_INET {
                    let ip = Ipv4Addr::from(u32::from_be(unsafe { sa.sin_addr.S_un.S_addr }));
                    let o = ip.octets();
                    out.push(format!("{}.{}.{}", o[0], o[1], o[2]));
                }
            }
            ua = u.Next;
        }
        adapter = a.Next;
    }
    out
}

/// Serveurs DNS des cartes de l'hôte (IPv4), sans doublons, ordre de préférence Windows.
pub fn host_dns_servers() -> Vec<String> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, GAA_FLAG_SKIP_UNICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u8> = vec![0; size as usize];
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_UNICAST;
    for _ in 0..3 {
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                flags,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if rc == 111 {
            buf.resize(size as usize, 0);
            continue;
        }
        if rc != 0 {
            return vec![];
        }
        break;
    }
    let mut out: Vec<String> = Vec::new();
    let mut adapter = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !adapter.is_null() {
        let a = unsafe { &*adapter };
        // OperStatus 1 = IfOperStatusUp ; on ignore les cartes inactives et les vEthernet HNS.
        if a.OperStatus.0 == 1 {
            let mut d = a.FirstDnsServerAddress;
            while !d.is_null() {
                let e = unsafe { &*d };
                if !e.Address.lpSockaddr.is_null() {
                    let sa = unsafe { &*(e.Address.lpSockaddr as *const SOCKADDR_IN) };
                    if sa.sin_family == AF_INET {
                        let ip = Ipv4Addr::from(u32::from_be(unsafe { sa.sin_addr.S_un.S_addr }))
                            .to_string();
                        if !out.contains(&ip)
                            && !ip.starts_with("127.")
                            && !ip.starts_with("172.30.")
                        {
                            out.push(ip);
                        }
                    }
                }
                d = e.Next;
            }
        }
        adapter = a.Next;
    }
    out
}

pub fn enumerate_networks() -> Result<Vec<String>> {
    let mut doc = PWSTR::null();
    let mut record = PWSTR::null();
    let r = unsafe { HcnEnumerateNetworks(&HSTRING::from("{}"), &mut doc, Some(&mut record)) };
    let record = take_doc(record);
    r.map_err(|e| hcn_err("HcnEnumerateNetworks", &e, record))?;
    let text = take_doc(doc).unwrap_or_else(|| "[]".into());
    let ids: Vec<String> = serde_json::from_str(&text)
        .map_err(|e| SolonError::internal(format!("liste HNS illisible : {e}")))?;
    Ok(ids)
}

fn network_properties(id: &str) -> Result<serde_json::Value> {
    let guid =
        GUID::try_from(id).map_err(|e| SolonError::internal(format!("GUID HNS {id} : {e}")))?;
    let mut handle: *mut c_void = std::ptr::null_mut();
    let mut record = PWSTR::null();
    unsafe { HcnOpenNetwork(&guid, &mut handle, Some(&mut record)) }
        .map_err(|e| hcn_err("HcnOpenNetwork", &e, take_doc(record)))?;
    let mut doc = PWSTR::null();
    let mut record = PWSTR::null();
    let r = unsafe {
        HcnQueryNetworkProperties(handle, &HSTRING::from("{}"), &mut doc, Some(&mut record))
    };
    unsafe {
        let _ = HcnCloseNetwork(handle);
    }
    let record = take_doc(record);
    r.map_err(|e| hcn_err("HcnQueryNetworkProperties", &e, record))?;
    let text = take_doc(doc).unwrap_or_else(|| "{}".into());
    serde_json::from_str(&text)
        .map_err(|e| SolonError::internal(format!("propriétés HNS illisibles : {e}")))
}

fn network_exists(id: &GUID) -> bool {
    let s = format!("{id:?}").to_lowercase();
    enumerate_networks()
        .map(|l| l.iter().any(|x| x.to_lowercase() == s))
        .unwrap_or(false)
}

/// Supprime le réseau Solon (et donc ses endpoints) s'il existe.
pub fn delete_network() -> Result<()> {
    if !network_exists(&NETWORK_ID) {
        return Ok(());
    }
    let mut record = PWSTR::null();
    unsafe { HcnDeleteNetwork(&NETWORK_ID, Some(&mut record)) }
        .map_err(|e| hcn_err("HcnDeleteNetwork", &e, take_doc(record)))
}

fn choose_prefix() -> Result<&'static str> {
    let used = used_prefixes();
    CANDIDATE_PREFIXES
        .iter()
        .copied()
        .find(|p| !used.iter().any(|u| u == p))
        .ok_or_else(|| {
            SolonError::new(
                ErrorCode::HcsError,
                format!(
                    "aucune plage IP libre parmi {CANDIDATE_PREFIXES:?} (utilisées : {used:?})"
                ),
            )
        })
}

/// Crée (ou recrée) le réseau Solon et un endpoint pour la machine. Renvoie tout ce que l'invité
/// doit savoir pour se configurer.
pub fn ensure_network_and_endpoint(vm_id: &str) -> Result<GuestNetwork> {
    // Un réseau ICS d'un démarrage précédent peut avoir un endpoint orphelin : on repart propre.
    delete_network()?;
    let prefix = choose_prefix()?;
    let gateway: Ipv4Addr = format!("{prefix}.1").parse().unwrap();
    let guest: Ipv4Addr = format!("{prefix}.2").parse().unwrap();

    let settings = serde_json::json!({
        "Name": NETWORK_NAME,
        "Type": "ICS",
        "Flags": 9,
        "IsolateSwitch": true,
        "IPv6": false,
        "Subnets": [{
            "AddressPrefix": format!("{prefix}.0/24"),
            "GatewayAddress": gateway.to_string(),
            "IpSubnets": [{ "IpAddressPrefix": format!("{prefix}.0/24") }]
        }]
    });
    let mut network: *mut c_void = std::ptr::null_mut();
    let mut record = PWSTR::null();
    unsafe {
        HcnCreateNetwork(
            &NETWORK_ID,
            &HSTRING::from(settings.to_string()),
            &mut network,
            Some(&mut record),
        )
    }
    .map_err(|e| hcn_err("HcnCreateNetwork", &e, take_doc(record)))?;

    // GUID distinct de celui de la machine : l'adressage HvSocket utilise l'identifiant de la
    // machine et un endpoint HNS homonyme pourrait le masquer (soupçon levé au bloc 2).
    let _ = vm_id;
    let endpoint_id = GUID::new().unwrap_or_default();
    let ep_settings = serde_json::json!({
        "VirtualNetwork": format!("{NETWORK_ID:?}"),
        "IPAddress": guest.to_string(),
        "PrefixLength": 24,
        "GatewayAddress": gateway.to_string(),
    });
    let mut endpoint: *mut c_void = std::ptr::null_mut();
    let mut record = PWSTR::null();
    let r = unsafe {
        HcnCreateEndpoint(
            network,
            &endpoint_id,
            &HSTRING::from(ep_settings.to_string()),
            &mut endpoint,
            Some(&mut record),
        )
    };
    let record = take_doc(record);
    if let Err(e) = r {
        unsafe {
            let _ = HcnCloseNetwork(network);
        }
        return Err(hcn_err("HcnCreateEndpoint", &e, record));
    }

    let mut doc = PWSTR::null();
    let mut record = PWSTR::null();
    let r = unsafe {
        HcnQueryEndpointProperties(endpoint, &HSTRING::from("{}"), &mut doc, Some(&mut record))
    };
    let record = take_doc(record);
    unsafe {
        let _ = HcnCloseEndpoint(endpoint);
        let _ = HcnCloseNetwork(network);
    }
    r.map_err(|e| hcn_err("HcnQueryEndpointProperties", &e, record))?;
    let props: serde_json::Value =
        serde_json::from_str(&take_doc(doc).unwrap_or_else(|| "{}".into())).unwrap_or_default();
    let mac = props
        .get("MacAddress")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_owned();
    let address = props
        .get("IPAddress")
        .and_then(|m| m.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(guest);
    let mut dns = host_dns_servers();
    if dns.is_empty() {
        dns = vec!["1.1.1.1".into(), "8.8.8.8".into()];
    }
    tracing::info!(%address, %gateway, mac, ?dns, "réseau HNS prêt");
    Ok(GuestNetwork {
        network_id: format!("{NETWORK_ID:?}").to_lowercase(),
        endpoint_id: format!("{endpoint_id:?}").to_lowercase(),
        mac_address: mac,
        address,
        prefix_len: 24,
        gateway,
        dns,
    })
}

pub fn delete_endpoint(endpoint_id: &str) -> Result<()> {
    let guid = GUID::try_from(endpoint_id)
        .map_err(|e| SolonError::internal(format!("GUID endpoint {endpoint_id} : {e}")))?;
    let mut record = PWSTR::null();
    match unsafe { HcnDeleteEndpoint(&guid, Some(&mut record)) } {
        Ok(()) => Ok(()),
        Err(e) => Err(hcn_err("HcnDeleteEndpoint", &e, take_doc(record))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrait_les_prefixes_d_un_document_hns() {
        let doc = serde_json::json!({"Subnets":[{"AddressPrefix":"172.30.0.0/24","IpSubnets":[{"IpAddressPrefix":"172.30.0.0/24"}]}],"Autre":{"AddressPrefix":"192.168.1.0/24"}});
        let mut p = extract_prefixes(&doc);
        p.sort();
        p.dedup();
        assert_eq!(p, vec!["172.30.0", "192.168.1"]);
    }
}
