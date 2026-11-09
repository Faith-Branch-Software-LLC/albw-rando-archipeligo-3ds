//! The protocol pieces of the albw.local page: HTTP requests, WebSocket frames, and mDNS answers.
//!
//! These are plain functions over bytes so they can be tested on a computer. The app owns the
//! sockets and calls these each frame.

/// The name the console answers to on the local network.
pub const HOST_NAME: &str = "albw.local";

/// One HTTP request, once its headers have all arrived.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// Where the body starts in the bytes read so far.
    pub body_start: usize,
    pub content_length: usize,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    /// True when the request asks to become a WebSocket.
    pub fn wants_websocket(&self) -> bool {
        self.header("Upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")) && self.header("Sec-WebSocket-Key").is_some()
    }
}

/// Parses a request from the bytes read so far. `None` means the headers have not all arrived;
/// `Err` means the bytes are not HTTP.
pub fn parse_request(data: &[u8]) -> Result<Option<Request>, ()> {
    let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") else {
        // Headers this large are not a browser talking to us.
        return if data.len() > 16 * 1024 { Err(()) } else { Ok(None) };
    };
    let head = std::str::from_utf8(&data[..end]).map_err(|_| ())?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().ok_or(())?.split(' ');
    let method = first.next().ok_or(())?.to_owned();
    let path = first.next().ok_or(())?.to_owned();
    if !first.next().is_some_and(|version| version.starts_with("HTTP/")) {
        return Err(());
    }
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let content_length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    Ok(Some(Request { method, path, headers, body_start: end + 4, content_length }))
}

/// The start of a response. The body follows, `length` bytes long.
pub fn response_head(status: &str, content_type: &str, length: u64, extra: &[(&str, &str)]) -> Vec<u8> {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {length}\r\nConnection: close\r\nCache-Control: no-store\r\n"
    );
    for (key, value) in extra {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    head.push_str("\r\n");
    head.into_bytes()
}

/// A whole short response.
pub fn response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = response_head(status, content_type, body.len() as u64, &[]);
    bytes.extend_from_slice(body);
    bytes
}

/// A file name safe to save on the SD card, keeping only the last path part.
pub fn safe_file_name(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next()?.trim();
    let name: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ' | '(' | ')') { c } else { '_' })
        .collect();
    (!name.is_empty() && !name.starts_with('.') && name.len() <= 120).then_some(name)
}

/// Undoes `%xx` escapes in a header or path value.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The reply that turns a request into a WebSocket.
pub fn websocket_accept(request: &Request) -> Option<Vec<u8>> {
    let key = request.header("Sec-WebSocket-Key")?;
    let accept = base64(&sha1(format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes()));
    Some(
        format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n")
            .into_bytes(),
    )
}

/// A WebSocket text message from the console. Server messages are never masked.
pub fn websocket_text(text: &str) -> Vec<u8> {
    let body = text.as_bytes();
    let mut frame = vec![0x81];
    match body.len() {
        0..=125 => frame.push(body.len() as u8),
        126..=0xffff => {
            frame.push(126);
            frame.extend_from_slice(&(body.len() as u16).to_be_bytes());
        }
        _ => {
            frame.push(127);
            frame.extend_from_slice(&(body.len() as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(body);
    frame
}

/// True if the bytes from a browser hold a WebSocket close frame. Other frames are ignored.
pub fn websocket_closed(data: &[u8]) -> bool {
    data.first().is_some_and(|byte| byte & 0x0f == 0x8)
}

fn dns_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

/// Reads a name at `at`, following compression pointers. Returns the name and where it ends.
fn read_name(packet: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..32 {
        let len = *packet.get(at)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(at + 1)));
        }
        if len & 0xc0 == 0xc0 {
            let pointer = ((len & 0x3f) << 8) | *packet.get(at + 1)? as usize;
            end.get_or_insert(at + 2);
            at = pointer;
            continue;
        }
        labels.push(String::from_utf8_lossy(packet.get(at + 1..at + 1 + len)?).into_owned());
        at += 1 + len;
    }
    None
}

/// An mDNS answer saying `name` is at `ip`.
pub fn mdns_announcement(name: &str, ip: [u8; 4]) -> Vec<u8> {
    let mut packet = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    packet.extend(dns_name(name));
    // Type A, class IN with the cache-flush bit, two minutes to live, four bytes of address.
    packet.extend_from_slice(&[0, 1, 0x80, 1, 0, 0, 0, 120, 0, 4]);
    packet.extend_from_slice(&ip);
    packet
}

/// The answer to an mDNS query, if it asks for `name`'s address.
pub fn mdns_answer(query: &[u8], name: &str, ip: [u8; 4]) -> Option<Vec<u8>> {
    // Only queries: the response bit must be clear.
    if query.len() < 12 || query[2] & 0x80 != 0 {
        return None;
    }
    let questions = u16::from_be_bytes([query[4], query[5]]);
    let mut at = 12;
    for _ in 0..questions {
        let (asked, end) = read_name(query, at)?;
        let kind = u16::from_be_bytes([*query.get(end)?, *query.get(end + 1)?]);
        at = end + 4;
        // Type A (1) or ANY (255).
        if asked.eq_ignore_ascii_case(name) && (kind == 1 || kind == 255) {
            return Some(mdns_announcement(name, ip));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_request_once_the_headers_arrive() {
        let partial = b"POST /patch HTTP/1.1\r\nContent-Length: 5\r\n";
        assert_eq!(parse_request(partial), Ok(None));
        let full = b"POST /patch HTTP/1.1\r\nContent-Length: 5\r\nX-File-Name: a%20b.apalbw3ds\r\n\r\nhello";
        let request = parse_request(full).unwrap().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/patch");
        assert_eq!(request.content_length, 5);
        assert_eq!(&full[request.body_start..], b"hello");
        assert_eq!(percent_decode(request.header("x-file-name").unwrap()), "a b.apalbw3ds");
        assert!(parse_request(b"\x16\x03\x01 not http\r\n\r\n").is_err());
    }

    #[test]
    fn websocket_handshake_matches_the_standard_example() {
        // The example key and answer from RFC 6455.
        let request = parse_request(
            b"GET /ws HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        )
        .unwrap()
        .unwrap();
        assert!(request.wants_websocket());
        let reply = String::from_utf8(websocket_accept(&request).unwrap()).unwrap();
        assert!(reply.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
        assert_eq!(websocket_text("hi"), [0x81, 2, b'h', b'i']);
        assert_eq!(&websocket_text(&"x".repeat(200))[..4], &[0x81, 126, 0, 200]);
    }

    #[test]
    fn file_names_stay_in_their_folder() {
        assert_eq!(safe_file_name("../../luma/AP_1.apalbw3ds").as_deref(), Some("AP_1.apalbw3ds"));
        assert_eq!(safe_file_name("C:\\Users\\me\\AP 1.apalbw3ds").as_deref(), Some("AP 1.apalbw3ds"));
        assert_eq!(safe_file_name("a<b>.apalbw3ds").as_deref(), Some("a_b_.apalbw3ds"));
        assert_eq!(safe_file_name(".hidden"), None);
        assert_eq!(safe_file_name(""), None);
    }

    #[test]
    fn answers_mdns_queries_for_its_name_only() {
        let mut query = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        query.extend(dns_name("albw.local"));
        query.extend_from_slice(&[0, 1, 0, 1]);
        let answer = mdns_answer(&query, HOST_NAME, [192, 168, 1, 40]).unwrap();
        assert_eq!(&answer[answer.len() - 4..], &[192, 168, 1, 40]);
        assert_eq!(answer[2], 0x84);

        let mut other = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        other.extend(dns_name("printer.local"));
        other.extend_from_slice(&[0, 1, 0, 1]);
        assert!(mdns_answer(&other, HOST_NAME, [192, 168, 1, 40]).is_none());
        // Its own announcement is a response, so it is not answered again.
        assert!(mdns_answer(&answer, HOST_NAME, [192, 168, 1, 40]).is_none());
    }
}
