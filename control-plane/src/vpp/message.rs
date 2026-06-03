use bytes::{Buf, BufMut, BytesMut};
use std::io;

/// VPP Binary API message header (16 bytes)
#[derive(Debug, Clone)]
pub struct VppMsgHeader {
    pub msg_type: u16,
    pub client_index: u32,
    pub context: u32,
}

impl VppMsgHeader {
    pub const SIZE: usize = 16;

    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_u16_le(self.msg_type);
        buf.put_u32_le(self.client_index);
        buf.put_u32_le(self.context);
        buf.put_u32_le(0); // padding
    }

    pub fn decode(buf: &[u8]) -> io::Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "header too short"));
        }
        let mut cursor = io::Cursor::new(buf);
        Ok(Self {
            msg_type: cursor.get_u16_le(),
            client_index: cursor.get_u32_le(),
            context: cursor.get_u32_le(),
        })
    }
}

/// VPP Binary API message
#[derive(Debug, Clone)]
pub struct VppMessage {
    pub header: VppMsgHeader,
    pub data: BytesMut,
}

impl VppMessage {
    /// Create a new request message
    pub fn new_request(msg_type: u16, context: u32) -> Self {
        Self {
            header: VppMsgHeader {
                msg_type,
                client_index: 0, // will be set by client
                context,
            },
            data: BytesMut::new(),
        }
    }

    /// Encode the message to bytes
    pub fn encode(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(VppMsgHeader::SIZE + self.data.len());
        self.header.encode(&mut buf);
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Get the message ID from a control_ping reply
    pub fn msg_type(&self) -> u16 {
        self.header.msg_type
    }

    pub fn context(&self) -> u32 {
        self.header.context
    }
}

/// Helper to encode VPP API string fields (fixed-size, null-terminated)
pub fn encode_vpp_string(s: &str, buf: &mut BytesMut, max_len: usize) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(max_len - 1);
    buf.put_slice(&bytes[..len]);
    // Pad with zeros
    for _ in len..max_len {
        buf.put_u8(0);
    }
}

/// Helper to decode VPP API string fields
pub fn decode_vpp_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}

/// VPP API return code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VppRetval(pub i32);

impl VppRetval {
    pub fn is_ok(&self) -> bool {
        self.0 == 0
    }

    pub fn error_message(&self) -> String {
        match self.0 {
            0 => "Success".to_string(),
            -1 => "Unspecified error".to_string(),
            -2 => "Invalid argument".to_string(),
            -3 => "Invalid value".to_string(),
            _ => format!("Error code: {}", self.0),
        }
    }
}

// VPP built-in message IDs
// These are fixed in VPP and don't change across versions
pub const MSG_GET_FIRST_MSG_ID: u16 = 15;
pub const MSG_GET_FIRST_MSG_ID_REPLY: u16 = 16;

/// Create a get_first_msg_id request
pub fn make_get_first_msg_id(name: &str, context: u32) -> VppMessage {
    let mut msg = VppMessage::new_request(MSG_GET_FIRST_MSG_ID, context);
    encode_vpp_string(name, &mut msg.data, 64);
    msg
}

/// Decode get_first_msg_id reply
pub fn decode_get_first_msg_id_reply(data: &[u8]) -> io::Result<u16> {
    if data.len() < 8 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "reply too short"));
    }
    let retval = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if retval != 0 {
        return Err(io::Error::new(io::ErrorKind::Other, format!("get_first_msg_id failed: {}", retval)));
    }
    let first_msg_id = u16::from_le_bytes([data[4], data[5]]);
    Ok(first_msg_id)
}
