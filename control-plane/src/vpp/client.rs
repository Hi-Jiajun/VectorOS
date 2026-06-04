use anyhow::{Context, Result};
use bytes::BytesMut;
use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::message::{
    VppMessage, VppMsgHeader, VppRetval,
    MSG_GET_FIRST_MSG_ID, MSG_GET_FIRST_MSG_ID_REPLY,
    make_get_first_msg_id, decode_get_first_msg_id_reply,
};

/// VPP Binary API Client
pub struct VppClient {
    stream: Mutex<UnixStream>,
    context_counter: AtomicU32,
}

unsafe impl Send for VppClient {}
unsafe impl Sync for VppClient {}

impl VppClient {
    /// Connect to VPP binary API socket
    pub fn connect(socket_path: &str) -> Result<Self> {
        info!("Connecting to VPP at {}", socket_path);
        let stream = UnixStream::connect(socket_path)
            .with_context(|| format!("Failed to connect to VPP socket: {}", socket_path))?;

        // Ensure blocking mode
        stream.set_nonblocking(false)?;
        // No timeout - let VPP take its time
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;

        let client = Self {
            stream: Mutex::new(stream),
            context_counter: AtomicU32::new(1),
        };

        info!("Connected to VPP");
        Ok(client)
    }

    /// Get next context ID
    pub fn next_context(&self) -> u32 {
        self.context_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a message and wait for reply
    pub fn send_recv(&self, mut msg: VppMessage) -> Result<VppMessage> {
        msg.header.client_index = 0;
        let context = msg.header.context;
        let msg_type = msg.msg_type();

        let encoded = msg.encode();
        let mut stream = self.stream.lock().unwrap();

        debug!("Sending message type={} context={} len={}", msg_type, context, encoded.len());
        stream.write_all(&encoded)?;
        stream.flush()?;

        // Read reply header (16 bytes)
        let mut header_buf = [0u8; VppMsgHeader::SIZE];
        stream.read_exact(&mut header_buf)?;
        let reply_header = VppMsgHeader::decode(&header_buf)?;

        debug!("Received reply type={} context={}", reply_header.msg_type, reply_header.context);

        // Read reply body based on message type
        let mut data = BytesMut::new();

        if reply_header.msg_type == MSG_GET_FIRST_MSG_ID_REPLY {
            // get_first_msg_id_reply: retval (4) + first_msg_id (2) = 6 bytes
            let mut body = [0u8; 6];
            stream.read_exact(&mut body)?;
            data.extend_from_slice(&body);
        } else if reply_header.msg_type == msg_type + 1 {
            // Standard reply (request + 1 = reply)
            // Body is typically: retval (4 bytes) + optional data
            let mut body = [0u8; 256]; // Read up to 256 bytes for reply
            match stream.read(&mut body) {
                Ok(n) => data.extend_from_slice(&body[..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    warn!("Timeout reading reply body");
                }
                Err(e) => return Err(e.into()),
            }
        } else {
            // Other message types - try to read some data
            let mut body = [0u8; 1024];
            match stream.read(&mut body) {
                Ok(n) => data.extend_from_slice(&body[..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    debug!("Timeout reading body (expected for some messages)");
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(VppMessage {
            header: reply_header,
            data,
        })
    }

    /// Send autoreply message and check retval
    pub fn send_autoreply(&self, msg: VppMessage) -> Result<VppRetval> {
        let reply = self.send_recv(msg)?;
        if reply.data.len() >= 4 {
            let retval = i32::from_le_bytes([
                reply.data[0],
                reply.data[1],
                reply.data[2],
                reply.data[3],
            ]);
            Ok(VppRetval(retval))
        } else {
            Ok(VppRetval(0))
        }
    }

    /// Get first message ID for a plugin
    pub fn get_first_msg_id(&self, plugin_name: &str) -> Result<u16> {
        let context = self.next_context();
        let msg = make_get_first_msg_id(plugin_name, context);
        let reply = self.send_recv(msg)?;

        if reply.msg_type() != MSG_GET_FIRST_MSG_ID_REPLY {
            anyhow::bail!(
                "Expected get_first_msg_id_reply ({}), got {}",
                MSG_GET_FIRST_MSG_ID_REPLY,
                reply.msg_type()
            );
        }

        Ok(decode_get_first_msg_id_reply(&reply.data)?)
    }

    /// Get all interfaces from VPP
    pub fn get_interfaces(&self) -> Result<Vec<InterfaceInfo>> {
        // TODO: Implement sw_interface_dump
        Ok(vec![])
    }
}

/// Network interface information from VPP
#[derive(Debug, Clone, serde::Serialize)]
pub struct InterfaceInfo {
    pub sw_if_index: u32,
    pub name: String,
    pub admin_up: bool,
    pub link_up: bool,
    pub mtu: u32,
}
