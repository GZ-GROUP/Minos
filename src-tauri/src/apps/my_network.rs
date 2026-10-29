use netdev::interface::state::OperState;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ipv4Info {
    address: String,
    netmask: String,
    prefix: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAdapter {
    index: u32,
    /// Friendly name when the OS provides one ("Wi-Fi"), otherwise the system name ("wlan0").
    name: String,
    description: Option<String>,
    kind: String,
    mac: Option<String>,
    ipv4: Vec<Ipv4Info>,
    /// "address/prefix" strings.
    ipv6: Vec<String>,
    gateway: Option<String>,
    dns_servers: Vec<String>,
    dhcp: Option<bool>,
    mtu: Option<u32>,
    speed_bps: Option<u64>,
    is_up: bool,
    is_default: bool,
}

#[tauri::command]
pub async fn network_adapters() -> Result<Vec<NetworkAdapter>, String> {
    let mut adapters: Vec<NetworkAdapter> = netdev::get_interfaces()
        .into_iter()
        .filter(|iface| !iface.is_loopback())
        .map(|iface| {
            let gateway = iface.gateway.as_ref().and_then(|device| {
                device
                    .ipv4
                    .first()
                    .map(ToString::to_string)
                    .or_else(|| device.ipv6.first().map(ToString::to_string))
            });

            NetworkAdapter {
                index: iface.index,
                name: iface
                    .friendly_name
                    .clone()
                    .unwrap_or_else(|| iface.name.clone()),
                description: iface.description.clone(),
                kind: iface.if_type.name(),
                mac: iface
                    .mac_addr
                    .map(|mac| mac.to_string().to_uppercase())
                    .filter(|mac| mac != "00:00:00:00:00:00"),
                ipv4: iface
                    .ipv4
                    .iter()
                    .map(|net| Ipv4Info {
                        address: net.addr().to_string(),
                        netmask: net.netmask().to_string(),
                        prefix: net.prefix_len(),
                    })
                    .collect(),
                ipv6: iface
                    .ipv6
                    .iter()
                    .map(|net| format!("{}/{}", net.addr(), net.prefix_len()))
                    .collect(),
                gateway,
                dns_servers: iface.dns_servers.iter().map(ToString::to_string).collect(),
                dhcp: iface.dhcp_v4_enabled,
                mtu: iface.mtu,
                speed_bps: iface.transmit_speed,
                is_up: iface.oper_state == OperState::Up,
                is_default: iface.default,
            }
        })
        .collect();

    // Default route first, then connected adapters, then by name.
    adapters.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then(b.is_up.cmp(&a.is_up))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(adapters)
}
