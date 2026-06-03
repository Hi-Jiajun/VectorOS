use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};

use super::client::VppClient;
use super::message::{VppMessage, encode_vpp_string};

// PPPoE client API message IDs (these need to be obtained from VPP at runtime)
// For now, we'll use placeholder values that need to be replaced with actual IDs
const MSG_PPPOECLIENT_ADD_DEL: u16 = 100; // placeholder
const MSG_PPPOECLIENT_SET_OPTIONS: u16 = 101;
const MSG_PPPOECLIENT_SESSION_ACTION: u16 = 102;
const MSG_PPPOECLIENT_DUMP: u16 = 103;
const MSG_PPPOECLIENT_DETAILS: u16 = 104;

/// PPPoE client state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PppoeClientState {
    Unknown = 0,
    Discovery = 1,
    Request = 2,
    Session = 3,
}

impl From<u8> for PppoeClientState {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Discovery,
            2 => Self::Request,
            3 => Self::Session,
            _ => Self::Unknown,
        }
    }
}

/// Session action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SessionAction {
    None = 0,
    Restart = 1,
    Stop = 2,
    Open = 3,
}

/// PPPoE client information from VPP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PppoeClientInfo {
    pub sw_if_index: u32,
    pub host_uniq: u32,
    pub pppox_sw_if_index: u32,
    pub pppox_unit: u32,
    pub pppox_session_allocated: bool,
    pub session_id: u16,
    pub client_state: PppoeClientState,
    pub ac_mac: [u8; 6],
    pub ac_name: String,
    pub configured_ac_name: String,
    pub service_name: String,

    // IPv4 addresses
    pub ipv4_local: [u8; 4],
    pub ipv4_peer: [u8; 4],
    pub peer_dns4_primary: [u8; 4],
    pub peer_dns4_secondary: [u8; 4],

    // IPv6 addresses
    pub peer_dns6_primary: [u8; 16],
    pub peer_dns6_secondary: [u8; 16],
    pub peer_dns6_count: u8,

    pub ipv6cp_local: [u8; 16],
    pub ipv6cp_peer: [u8; 16],
    pub wan_ipv6: [u8; 16],
    pub wan_ipv6_prefix_len: u8,
    pub wan_ipv6_observed: bool,

    // Options
    pub use_peer_dns: bool,
    pub add_default_route4: bool,
    pub add_default_route6: bool,
    pub auth_user: String,
    pub mtu: u32,
    pub mru: u32,
    pub timeout: u32,

    // Statistics
    pub total_reconnects: u32,
    pub last_disconnect_reason: u8,
    pub session_uptime_seconds: u32,
    pub consecutive_auth_failures: u32,

    // DHCPv6 state
    pub dhcp6_ia_na_enabled: bool,
    pub dhcp6_ia_na_rebinding: bool,
    pub dhcp6_pd_enabled: bool,
    pub dhcp6_pd_rebinding: bool,
}

/// Request to add/delete a PPPoE client
pub struct PppoeClientAddDel {
    pub is_add: bool,
    pub sw_if_index: u32,
    pub host_uniq: u32,
    pub configured_ac_name: String,
    pub service_name: String,
    pub custom_ifname: String,
}

impl PppoeClientAddDel {
    /// Encode as VPP binary API message
    pub fn encode(&self, context: u32) -> VppMessage {
        let mut msg = VppMessage::new_request(MSG_PPPOECLIENT_ADD_DEL, context);
        msg.data.put_u8(if self.is_add { 1 } else { 0 });
        msg.data.put_u32_le(self.sw_if_index);
        msg.data.put_u32_le(self.host_uniq);
        encode_vpp_string(&self.configured_ac_name, &mut msg.data, 64);
        encode_vpp_string(&self.service_name, &mut msg.data, 64);
        encode_vpp_string(&self.custom_ifname, &mut msg.data, 64);
        msg
    }
}

/// Request to set PPPoE client options
pub struct PppoeClientSetOptions {
    pub pppoeclient_index: u32,
    pub username: String,
    pub password: String,
    pub use_peer_dns: bool,
    pub add_default_route4: bool,
    pub add_default_route6: bool,
    pub mtu: u32,
    pub mru: u32,
    pub timeout: u32,
    pub set_use_peer_dns: bool,
    pub set_add_default_route4: bool,
    pub set_add_default_route6: bool,
    pub configured_ac_name: String,
    pub service_name: String,
    pub clear_ac_name: bool,
    pub clear_service_name: bool,
}

impl PppoeClientSetOptions {
    pub fn encode(&self, context: u32) -> VppMessage {
        let mut msg = VppMessage::new_request(MSG_PPPOECLIENT_SET_OPTIONS, context);
        msg.data.put_u32_le(self.pppoeclient_index);
        encode_vpp_string(&self.username, &mut msg.data, 64);
        encode_vpp_string(&self.password, &mut msg.data, 64);
        msg.data.put_u8(if self.use_peer_dns { 1 } else { 0 });
        msg.data.put_u8(if self.add_default_route4 { 1 } else { 0 });
        msg.data.put_u8(if self.add_default_route6 { 1 } else { 0 });
        msg.data.put_u32_le(self.mtu);
        msg.data.put_u32_le(self.mru);
        msg.data.put_u32_le(self.timeout);
        msg.data.put_u8(if self.set_use_peer_dns { 1 } else { 0 });
        msg.data.put_u8(if self.set_add_default_route4 { 1 } else { 0 });
        msg.data.put_u8(if self.set_add_default_route6 { 1 } else { 0 });
        encode_vpp_string(&self.configured_ac_name, &mut msg.data, 64);
        encode_vpp_string(&self.service_name, &mut msg.data, 64);
        msg.data.put_u8(if self.clear_ac_name { 1 } else { 0 });
        msg.data.put_u8(if self.clear_service_name { 1 } else { 0 });
        msg
    }
}

/// Request to perform session action
pub struct PppoeClientSessionAction {
    pub pppoeclient_index: u32,
    pub action: SessionAction,
}

impl PppoeClientSessionAction {
    pub fn encode(&self, context: u32) -> VppMessage {
        let mut msg = VppMessage::new_request(MSG_PPPOECLIENT_SESSION_ACTION, context);
        msg.data.put_u32_le(self.pppoeclient_index);
        msg.data.put_u8(self.action as u8);
        msg
    }
}

/// Decode pppoeclient_details reply
pub fn decode_pppoeclient_details(data: &[u8]) -> Option<PppoeClientInfo> {
    if data.len() < 4 {
        return None;
    }

    let mut cursor = std::io::Cursor::new(data);

    // Skip retval (first 4 bytes in reply)
    let retval = cursor.get_i32_le();
    if retval != 0 {
        return None;
    }

    // Decode fields according to pppoeclient.api
    let sw_if_index = cursor.get_u32_le();
    let host_uniq = cursor.get_u32_le();
    let pppox_sw_if_index = cursor.get_u32_le();
    let pppox_unit = cursor.get_u32_le();
    let pppox_session_allocated = cursor.get_u8() != 0;
    let session_id = cursor.get_u16_le();
    let client_state = PppoeClientState::from(cursor.get_u8());

    // AC MAC (6 bytes)
    let mut ac_mac = [0u8; 6];
    cursor.copy_to_slice(&mut ac_mac);

    // Strings (64 bytes each)
    let ac_name = read_vpp_string(&mut cursor, 64);
    let configured_ac_name = read_vpp_string(&mut cursor, 64);
    let service_name = read_vpp_string(&mut cursor, 64);

    // IPv4 addresses (4 bytes each)
    let mut ipv4_local = [0u8; 4];
    cursor.copy_to_slice(&mut ipv4_local);
    let mut ipv4_peer = [0u8; 4];
    cursor.copy_to_slice(&mut ipv4_peer);
    let mut peer_dns4_primary = [0u8; 4];
    cursor.copy_to_slice(&mut peer_dns4_primary);
    let mut peer_dns4_secondary = [0u8; 4];
    cursor.copy_to_slice(&mut peer_dns4_secondary);

    // IPv6 addresses (16 bytes each)
    let mut peer_dns6_primary = [0u8; 16];
    cursor.copy_to_slice(&mut peer_dns6_primary);
    let mut peer_dns6_secondary = [0u8; 16];
    cursor.copy_to_slice(&mut peer_dns6_secondary);
    let peer_dns6_count = cursor.get_u8();

    let mut ipv6cp_local = [0u8; 16];
    cursor.copy_to_slice(&mut ipv6cp_local);
    let mut ipv6cp_peer = [0u8; 16];
    cursor.copy_to_slice(&mut ipv6cp_peer);
    let mut wan_ipv6 = [0u8; 16];
    cursor.copy_to_slice(&mut wan_ipv6);
    let wan_ipv6_prefix_len = cursor.get_u8();
    let wan_ipv6_observed = cursor.get_u8() != 0;

    // Skip some fields we don't need immediately
    let _peer_host_route6 = cursor.get_u8();
    let _use_peer_ipv6 = cursor.get_u8();

    let use_peer_dns = cursor.get_u8() != 0;
    let add_default_route4 = cursor.get_u8() != 0;
    let add_default_route6 = cursor.get_u8() != 0;
    let auth_user = read_vpp_string(&mut cursor, 64);
    let mtu = cursor.get_u32_le();
    let mru = cursor.get_u32_le();
    let timeout = cursor.get_u32_le();

    // Statistics
    let total_reconnects = cursor.get_u32_le();
    let last_disconnect_reason = cursor.get_u8();
    let session_uptime_seconds = cursor.get_u32_le();
    let consecutive_auth_failures = cursor.get_u32_le();

    // DHCPv6 state
    let dhcp6_ia_na_enabled = cursor.get_u8() != 0;
    let dhcp6_ia_na_rebinding = cursor.get_u8() != 0;
    // Skip more DHCPv6 fields for now
    let dhcp6_pd_enabled = cursor.get_u8() != 0;
    let dhcp6_pd_rebinding = cursor.get_u8() != 0;

    Some(PppoeClientInfo {
        sw_if_index,
        host_uniq,
        pppox_sw_if_index,
        pppox_unit,
        pppox_session_allocated,
        session_id,
        client_state,
        ac_mac,
        ac_name,
        configured_ac_name,
        service_name,
        ipv4_local,
        ipv4_peer,
        peer_dns4_primary,
        peer_dns4_secondary,
        peer_dns6_primary,
        peer_dns6_secondary,
        peer_dns6_count,
        ipv6cp_local,
        ipv6cp_peer,
        wan_ipv6,
        wan_ipv6_prefix_len,
        wan_ipv6_observed,
        use_peer_dns,
        add_default_route4,
        add_default_route6,
        auth_user,
        mtu,
        mru,
        timeout,
        total_reconnects,
        last_disconnect_reason,
        session_uptime_seconds,
        consecutive_auth_failures,
        dhcp6_ia_na_enabled,
        dhcp6_ia_na_rebinding,
        dhcp6_pd_enabled,
        dhcp6_pd_rebinding,
    })
}

/// Helper to read a fixed-size VPP string from cursor
fn read_vpp_string(cursor: &mut std::io::Cursor<&[u8]>, max_len: usize) -> String {
    let pos = cursor.position() as usize;
    let remaining = cursor.get_ref();
    let end = pos + max_len;
    if end > remaining.len() {
        cursor.set_position(remaining.len() as u64);
        return String::new();
    }
    let slice = &remaining[pos..end];
    let null_pos = slice.iter().position(|&b| b == 0).unwrap_or(max_len);
    let s = String::from_utf8_lossy(&slice[..null_pos]).to_string();
    cursor.set_position(end as u64);
    s
}

/// High-level PPPoE operations
impl VppClient {
    /// Create a PPPoE client
    pub fn pppoe_add_client(
        &self,
        sw_if_index: u32,
        host_uniq: u32,
        ac_name: &str,
        service_name: &str,
        ifname: &str,
    ) -> Result<u32, anyhow::Error> {
        let context = self.next_context();
        let _msg = PppoeClientAddDel {
            is_add: true,
            sw_if_index,
            host_uniq,
            configured_ac_name: ac_name.to_string(),
            service_name: service_name.to_string(),
            custom_ifname: ifname.to_string(),
        }
        .encode(context);

        // TODO: Send and parse reply
        // The reply contains pppox_sw_if_index on success
        anyhow::bail!("Not yet implemented - need message ID lookup")
    }

    /// Set PPPoE client options (username, password, etc.)
    pub fn pppoe_set_options(
        &self,
        pppoeclient_index: u32,
        username: &str,
        password: &str,
        mtu: u32,
        mru: u32,
    ) -> Result<(), anyhow::Error> {
        let context = self.next_context();
        let _msg = PppoeClientSetOptions {
            pppoeclient_index,
            username: username.to_string(),
            password: password.to_string(),
            use_peer_dns: true,
            add_default_route4: true,
            add_default_route6: true,
            mtu,
            mru,
            timeout: 10,
            set_use_peer_dns: true,
            set_add_default_route4: true,
            set_add_default_route6: true,
            configured_ac_name: String::new(),
            service_name: String::new(),
            clear_ac_name: false,
            clear_service_name: false,
        }
        .encode(context);

        // TODO: Send and check retval
        anyhow::bail!("Not yet implemented - need message ID lookup")
    }

    /// Perform session action (restart/stop/open)
    pub fn pppoe_session_action(
        &self,
        pppoeclient_index: u32,
        action: SessionAction,
    ) -> Result<(), anyhow::Error> {
        let context = self.next_context();
        let _msg = PppoeClientSessionAction {
            pppoeclient_index,
            action,
        }
        .encode(context);

        // TODO: Send and check retval
        anyhow::bail!("Not yet implemented - need message ID lookup")
    }

    /// Dump all PPPoE clients
    pub fn pppoe_dump_clients(&self) -> Result<Vec<PppoeClientInfo>, anyhow::Error> {
        // TODO: Implement dump
        anyhow::bail!("Not yet implemented - need message ID lookup")
    }

    /// Get details of a specific PPPoE client
    pub fn pppoe_get_client(&self, _sw_if_index: u32) -> Result<Option<PppoeClientInfo>, anyhow::Error> {
        // TODO: Implement dump with filter
        anyhow::bail!("Not yet implemented - need message ID lookup")
    }
}
