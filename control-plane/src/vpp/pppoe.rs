use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};

use super::client::VppClient;
use super::message::{VppMessage, VppRetval, encode_vpp_string};

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

/// Message ID offsets for pppoeclient plugin
/// These are relative to the first message ID returned by get_first_msg_id
const PPPOECLIENT_ADD_DEL: u8 = 0;
const PPPOECLIENT_ADD_DEL_REPLY: u8 = 1;
const PPPOECLIENT_SET_OPTIONS: u8 = 2;
const PPPOECLIENT_SESSION_ACTION: u8 = 3;
const PPPOECLIENT_DUMP: u8 = 4;
const PPPOECLIENT_DETAILS: u8 = 5;
// const PPPOECLIENT_CONTROL_HISTORY_DUMP: u8 = 6;
// const PPPOECLIENT_CONTROL_HISTORY_DETAILS: u8 = 7;
// const PPPOECLIENT_CONTROL_HISTORY_CLEAR: u8 = 8;
// const PPPOECLIENT_CONTROL_HISTORY_SUMMARY: u8 = 9;
// const PPPOECLIENT_CONTROL_HISTORY_SUMMARY_REPLY: u8 = 10;

/// PPPoE client API with dynamic message IDs
pub struct PppoeApi {
    base_msg_id: u16,
}

impl PppoeApi {
    /// Initialize PPPoE API by getting message IDs from VPP
    pub fn init(client: &VppClient) -> Result<Self, anyhow::Error> {
        let base_msg_id = client.get_first_msg_id("pppoeclient")?;
        tracing::info!("PPPoE client base message ID: {}", base_msg_id);
        Ok(Self { base_msg_id })
    }

    fn msg_id(&self, offset: u8) -> u16 {
        self.base_msg_id + offset as u16
    }

    /// Create a PPPoE client add/del message
    pub fn make_add_del(
        &self,
        is_add: bool,
        sw_if_index: u32,
        host_uniq: u32,
        ac_name: &str,
        service_name: &str,
        ifname: &str,
        context: u32,
    ) -> VppMessage {
        let mut msg = VppMessage::new_request(self.msg_id(PPPOECLIENT_ADD_DEL), context);
        msg.data.put_u8(if is_add { 1 } else { 0 });
        msg.data.put_u32_le(sw_if_index);
        msg.data.put_u32_le(host_uniq);
        encode_vpp_string(ac_name, &mut msg.data, 64);
        encode_vpp_string(service_name, &mut msg.data, 64);
        encode_vpp_string(ifname, &mut msg.data, 64);
        msg
    }

    /// Create a PPPoE client set_options message
    pub fn make_set_options(
        &self,
        pppoeclient_index: u32,
        username: &str,
        password: &str,
        use_peer_dns: bool,
        add_default_route4: bool,
        add_default_route6: bool,
        mtu: u32,
        mru: u32,
        timeout: u32,
        context: u32,
    ) -> VppMessage {
        let mut msg = VppMessage::new_request(self.msg_id(PPPOECLIENT_SET_OPTIONS), context);
        msg.data.put_u32_le(pppoeclient_index);
        encode_vpp_string(username, &mut msg.data, 64);
        encode_vpp_string(password, &mut msg.data, 64);
        msg.data.put_u8(if use_peer_dns { 1 } else { 0 });
        msg.data.put_u8(if add_default_route4 { 1 } else { 0 });
        msg.data.put_u8(if add_default_route6 { 1 } else { 0 });
        msg.data.put_u32_le(mtu);
        msg.data.put_u32_le(mru);
        msg.data.put_u32_le(timeout);
        msg.data.put_u8(if use_peer_dns { 1 } else { 0 }); // set_use_peer_dns
        msg.data.put_u8(if add_default_route4 { 1 } else { 0 }); // set_add_default_route4
        msg.data.put_u8(if add_default_route6 { 1 } else { 0 }); // set_add_default_route6
        encode_vpp_string("", &mut msg.data, 64); // configured_ac_name
        encode_vpp_string("", &mut msg.data, 64); // service_name
        msg.data.put_u8(0); // clear_ac_name
        msg.data.put_u8(0); // clear_service_name
        msg
    }

    /// Create a PPPoE client session_action message
    pub fn make_session_action(
        &self,
        pppoeclient_index: u32,
        action: SessionAction,
        context: u32,
    ) -> VppMessage {
        let mut msg = VppMessage::new_request(self.msg_id(PPPOECLIENT_SESSION_ACTION), context);
        msg.data.put_u32_le(pppoeclient_index);
        msg.data.put_u8(action as u8);
        msg
    }

    /// Create a PPPoE client dump message
    pub fn make_dump(&self, sw_if_index: u32, context: u32) -> VppMessage {
        let mut msg = VppMessage::new_request(self.msg_id(PPPOECLIENT_DUMP), context);
        msg.data.put_u32_le(sw_if_index);
        msg
    }

    /// Check if a message is a pppoeclient_details reply
    pub fn is_details_reply(&self, msg_type: u16) -> bool {
        msg_type == self.msg_id(PPPOECLIENT_DETAILS)
    }

    /// Decode pppoeclient_details reply
    pub fn decode_details(&self, data: &[u8]) -> Option<PppoeClientInfo> {
        decode_pppoeclient_details(data)
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
    /// Get PPPoE API instance
    pub fn pppoe_api(&self) -> Result<PppoeApi, anyhow::Error> {
        PppoeApi::init(self)
    }

    /// Create a PPPoE client
    pub fn pppoe_add_client(
        &self,
        pppoe: &PppoeApi,
        sw_if_index: u32,
        host_uniq: u32,
        ac_name: &str,
        service_name: &str,
        ifname: &str,
    ) -> Result<u32, anyhow::Error> {
        let context = self.next_context();
        let msg = pppoe.make_add_del(true, sw_if_index, host_uniq, ac_name, service_name, ifname, context);
        let reply = self.send_recv(msg)?;

        if reply.data.len() >= 8 {
            let retval = i32::from_le_bytes([
                reply.data[0], reply.data[1], reply.data[2], reply.data[3],
            ]);
            if retval != 0 {
                anyhow::bail!("pppoeclient_add_del failed: {}", retval);
            }
            let pppox_sw_if_index = u32::from_le_bytes([
                reply.data[4], reply.data[5], reply.data[6], reply.data[7],
            ]);
            Ok(pppox_sw_if_index)
        } else {
            anyhow::bail!("pppoeclient_add_del reply too short");
        }
    }

    /// Set PPPoE client options
    pub fn pppoe_set_options(
        &self,
        pppoe: &PppoeApi,
        pppoeclient_index: u32,
        username: &str,
        password: &str,
        mtu: u32,
        mru: u32,
    ) -> Result<(), anyhow::Error> {
        let context = self.next_context();
        let msg = pppoe.make_set_options(
            pppoeclient_index, username, password,
            true, true, true, // use_peer_dns, add_default_route4, add_default_route6
            mtu, mru, 10, // timeout
            context,
        );
        let retval = self.send_autoreply(msg)?;
        if !retval.is_ok() {
            anyhow::bail!("pppoeclient_set_options failed: {}", retval.error_message());
        }
        Ok(())
    }

    /// Perform session action
    pub fn pppoe_session_action(
        &self,
        pppoe: &PppoeApi,
        pppoeclient_index: u32,
        action: SessionAction,
    ) -> Result<(), anyhow::Error> {
        let context = self.next_context();
        let msg = pppoe.make_session_action(pppoeclient_index, action, context);
        let retval = self.send_autoreply(msg)?;
        if !retval.is_ok() {
            anyhow::bail!("pppoeclient_session_action failed: {}", retval.error_message());
        }
        Ok(())
    }

    /// Dump all PPPoE clients (simplified - single client)
    pub fn pppoe_dump_clients(&self, pppoe: &PppoeApi) -> Result<Vec<PppoeClientInfo>, anyhow::Error> {
        // TODO: Implement proper dump with control_ping
        // For now, return empty - we need to handle the dump/details protocol
        Ok(vec![])
    }
}
