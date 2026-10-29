//! Minimal RFC 6455 server-side text-frame support for the loopback WPS bridge.
use std::io::{Read, Write};
use std::net::TcpStream;

use sha1::{Digest, Sha1};

pub const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn accept_key(key: &str) -> String {
    let mut digest = Sha1::new();
    digest.update(key.as_bytes());
    digest.update(GUID.as_bytes());
    base64(&digest.finalize())
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (a << 16) | (b << 8) | c;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((value >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(value & 63) as usize] as char } else { '=' });
    }
    out
}

pub fn handshake(stream: &mut TcpStream, key: &str) -> std::io::Result<()> {
    let accept = accept_key(key);
    stream.write_all(format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    ).as_bytes())?;
    stream.flush()
}

pub struct Frame {
    pub opcode: u8,
    pub payload: Vec<u8>,
}

/// Reads one complete unfragmented client frame. The caller bounds the payload
/// to keep the bridge small; extended lengths and fragmentation are rejected.
pub fn read_frame(stream: &mut TcpStream) -> std::io::Result<Frame> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    if !masked { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unmasked client frame")); }
    let length = (header[1] & 0x7f) as usize;
    if length >= 126 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported frame length")); }
    let mut mask = [0u8; 4];
    stream.read_exact(&mut mask)?;
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload)?;
    for (index, byte) in payload.iter_mut().enumerate() { *byte ^= mask[index % 4]; }
    Ok(Frame { opcode, payload })
}

pub fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > 125 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large")); }
    stream.write_all(&[0x80 | opcode, payload.len() as u8])?;
    stream.write_all(payload)?;
    stream.flush()
}

pub fn write_text(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    write_frame(stream, 0x1, text.as_bytes())
}

pub fn write_close(stream: &mut TcpStream) {
    let _ = write_frame(stream, 0x8, &[]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc_handshake_vector() {
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }
    #[test]
    fn base64_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
}
