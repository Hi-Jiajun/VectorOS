use anyhow::{Context, Result};
use bytes::BytesMut;
use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tracing::{debug, info};

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

// UnixStream with Mutex is Send + Sync
unsafe impl Send for VppClient {}
unsafe impl Sync for VppClient {}

impl VppClient {
    /// Connect to VPP binary API socket
    pub fn connect(socket_path: &str) -> Result<Self> {
        info!("Connecting to VPP at {}", socket_path);
        let stream = UnixStream::connect(socket_path)
            .with_context(|| format!("Failed to connect to VPP socket: {}", socket_path))?;

        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

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

        debug!("Sending message type={} context={}", msg_type, context);
        stream.write_all(&encoded)?;

        // Read reply header
        let mut header_buf = vec![0u8; VppMsgHeader::SIZE];
        stream.read_exact(&mut header_buf)?;
        let reply_header = VppMsgHeader::decode(&header_buf)?;

        debug!("Received reply type={} context={}", reply_header.msg_type, reply_header.context);

        // For autoreply messages, the body is just retval (4 bytes)
        // For dump messages, we get multiple details messages followed by a control_ping_reply
        let mut data = BytesMut::with_capacity(8192);

        // Read body based on message type
        // For now, read a fixed amount for replies
        if reply_header.msg_type == MSG_GET_FIRST_MSG_ID_REPLY {
            // get_first_msg_id_reply: retval (4) + first_msg_id (2)
            let mut body = [0u8; 6];
            stream.read_exact(&mut body)?;
            data.extend_from_slice(&body);
        } else if reply_header.msg_type <= 2 {
            // Control ping reply or similar - read retval
            let mut body = [0u8; 4];
            stream.read_exact(&mut body)?;
            data.extend_from_slice(&body);
        } else {
            // For other messages, try to read available data
            // TODO: Implement proper message length parsing
            stream.set_nonblocking(true)?;
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => data.extend_from_slice(&buf[..n]),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        stream.set_nonblocking(false)?;
                        return Err(e.into());
                    }
                }
            }
            stream.set_nonblocking(false)?;
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
