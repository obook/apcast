//! rtsp.rs
//! One RTSP connection to a receiver. The receiver keeps the pairing state per connection:
//! info, pairing and the session must all go through the same one.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::Context;

const TIMEOUT: Duration = Duration::from_secs(5);

pub struct Rtsp {
    stream: BufReader<TcpStream>,
    cseq: u32,
}

/// An answer: its status code and body.
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Rtsp {
    /// Connect to the receiver's control port. UxPlay (the test receiver) serves on the port
    /// below the one it announces (seen 2026-10-22): that one is tried when the first refuses.
    /// ponytail: a guess on the port, browse `_raop._tcp` for the real one if a receiver needs it.
    pub fn connect(receiver: SocketAddr) -> anyhow::Result<Self> {
        let below = SocketAddr::new(receiver.ip(), receiver.port().saturating_sub(1));
        let stream = TcpStream::connect_timeout(&receiver, TIMEOUT)
            .or_else(|e| if e.kind() == std::io::ErrorKind::ConnectionRefused { TcpStream::connect_timeout(&below, TIMEOUT) } else { Err(e) })
            .with_context(|| format!("cannot reach {receiver}"))?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        Ok(Self { stream: BufReader::new(stream), cseq: 0 })
    }

    /// Send `method path` with `body` of type `content_type`, and read the answer.
    pub fn request(&mut self, method: &str, path: &str, content_type: &str, body: &[u8]) -> anyhow::Result<Answer> {
        self.cseq += 1;
        let version = env!("CARGO_PKG_VERSION");
        let mut head = format!(
            "{method} {path} RTSP/1.0\r\nCSeq: {}\r\nX-Apple-ProtocolVersion: 1\r\nUser-Agent: sacast/{version}\r\n",
            self.cseq
        );
        if !body.is_empty() {
            head.push_str(&format!("Content-Type: {content_type}\r\nContent-Length: {}\r\n", body.len()));
        }
        head.push_str("\r\n");
        let socket = self.stream.get_mut();
        socket.write_all(head.as_bytes())?;
        socket.write_all(body)?;
        read_answer(&mut self.stream).with_context(|| format!("{method} {path}"))
    }

    /// `request`, failing unless the status is 200.
    pub fn ok(&mut self, method: &str, path: &str, content_type: &str, body: &[u8]) -> anyhow::Result<Vec<u8>> {
        let answer = self.request(method, path, content_type, body)?;
        anyhow::ensure!(answer.status == 200, "{method} {path}: the receiver answered {}", answer.status);
        Ok(answer.body)
    }
}

/// Status line, headers, then a body of Content-Length bytes.
fn read_answer(reader: &mut impl BufRead) -> anyhow::Result<Answer> {
    let mut line = String::new();
    anyhow::ensure!(reader.read_line(&mut line)? > 0, "the receiver closed the connection");
    let status = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).with_context(|| format!("bad status line {:?}", line.trim()))?;
    let mut length = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().context("bad Content-Length")?;
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Answer { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_answers() {
        let mut two = std::io::Cursor::new(b"RTSP/1.0 200 OK\r\nContent-Length: 3\r\n\r\nabcRTSP/1.0 470 Client Authentication Failure\r\nCSeq: 2\r\n\r\n".to_vec());
        let first = read_answer(&mut two).unwrap();
        assert_eq!((first.status, first.body.as_slice()), (200, &b"abc"[..]));
        let second = read_answer(&mut two).unwrap();
        assert_eq!((second.status, second.body.len()), (470, 0));
        assert!(read_answer(&mut two).is_err());
    }
}
