//! The albw.local page: a small web server and mDNS responder that run while the sharing page is open.
//!
//! Everything is non-blocking and polled once a frame from the main loop, because 3DS threads only
//! switch when one waits. The protocol details live in `ui_core::net`.

use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::path::Path;

use ctru::services::soc::Soc;
use ui_core::net;

use crate::job;

/// The apworld the page offers, kept beside the app's other files.
pub const APWORLD: &str = "sdmc:/3ds/albw-rando/apworld/albw_3ds.apworld";
/// The apworld's version, written next to it.
pub const APWORLD_VERSION: &str = "sdmc:/3ds/albw-rando/apworld/version.txt";

const PAGE: &str = include_str!("share_page.html");
const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
/// Patch files are tens of kilobytes, and the apworld a few megabytes. Anything this large is
/// neither.
const MOST_UPLOAD: usize = 32 * 1024 * 1024;
/// How much one connection reads in a frame, so a large upload does not stall the screen.
const MOST_READ_PER_FRAME: usize = 512 * 1024;
/// While a request is arriving, keep reading for this long each frame instead of stopping at the
/// first empty read. Reading only once a frame let each frame take in just one small buffer's
/// worth, which made a few megabytes take minutes.
const UPLOAD_TIME_PER_FRAME: std::time::Duration = std::time::Duration::from_millis(10);
/// A larger receive buffer lets the computer send more before waiting for the 3DS. ftpd uses the
/// same size on the 3DS.
const RECEIVE_BUFFER: libc::c_int = 32 * 1024;
/// Where releases are published.
pub const REPOSITORY: &str = "Faith-Branch-Software-LLC/albw-rando-archipeligo-3ds";
const CHUNK: usize = 16 * 1024;
/// Announce the name again now and then, about every 30 seconds at 60 frames a second.
const ANNOUNCE_EVERY: u32 = 1800;

/// What the page serves, refreshed by the app.
pub struct Content {
    pub player: String,
    pub options_file: String,
    /// The version of the apworld on the SD card, from `apworld_version`.
    pub apworld: Option<String>,
}

/// Something the menu should hear about.
pub enum Event {
    /// A patch arrived and was saved under this name.
    Saved(String),
    /// An apworld arrived and was saved. It has this version.
    Apworld(String),
}

enum State {
    /// Reading the request, and its body once the headers say how long it is.
    Reading(Vec<u8>),
    /// Sending a reply, then perhaps a file after it, then closing.
    Writing { data: Vec<u8>, sent: usize, file: Option<File> },
    /// An open WebSocket that gets status messages.
    Socket { out: Vec<u8> },
    /// Everything is sent and our side is shut. Waits for the browser to close its side, because
    /// closing at once can throw away the end of the reply, and the browser then shows an error.
    Closing { frames: u32 },
    /// Done. Dropped at the end of the frame.
    Closed,
}

/// How long a closing connection waits for the browser, about 3 seconds at 60 frames a second.
const CLOSE_WAIT_FRAMES: u32 = 180;
/// Browsers open several connections at once, some of them spare ones that may never be used.
const MOST_CONNECTIONS: usize = 16;

struct Connection {
    stream: TcpStream,
    state: State,
    idle: u32,
    /// When the connection opened, to log how long uploads take.
    opened: std::time::Instant,
    /// A file on its way in: what it is, the bytes of it read so far, and its size.
    upload: Option<(&'static str, usize, usize)>,
}

/// A file arriving from the computer, for the sharing page's bar.
pub struct Upload {
    /// "patch" or "apworld".
    pub kind: &'static str,
    pub received: usize,
    pub total: usize,
}

pub struct Share {
    // Dropping the service closes every socket, so it goes last.
    listener: TcpListener,
    mdns: Option<UdpSocket>,
    connections: Vec<Connection>,
    ip: Ipv4Addr,
    frames: u32,
    last_status: String,
    _soc: Soc,
}

/// The version of the apworld on the SD card, or None when it is missing.
pub fn apworld_version() -> Option<String> {
    if !Path::new(APWORLD).exists() {
        return None;
    }
    Some(fs::read_to_string(APWORLD_VERSION).map(|text| text.trim().to_owned()).unwrap_or_else(|_| "unknown".to_owned()))
}

fn status_json(content: &Content) -> String {
    serde_json::json!({
        "player": content.player,
        "apworld": content.apworld,
        "needs": ap_patch::apworld_version_needed(),
        "download": release_link(),
        "app": env!("CARGO_PKG_VERSION"),
        "repository": REPOSITORY,
    })
    .to_string()
}

/// The release tag this build came from. The release workflow sets it.
fn release_tag() -> Option<&'static str> {
    option_env!("ALBW_RELEASE_TAG").filter(|tag| !tag.is_empty())
}

/// Where a computer can download the apworld that matches this app.
pub fn release_link() -> String {
    match release_tag() {
        Some(tag) => format!("https://github.com/{REPOSITORY}/releases/download/{tag}/albw_3ds.apworld"),
        None => format!("https://github.com/{REPOSITORY}/releases/latest"),
    }
}

impl Share {
    /// Starts sharing. Fails with a message the menu can show.
    pub fn start() -> Result<Self, String> {
        let soc = Soc::new().map_err(|err| format!("The network could not start: {err}"))?;
        let ip = soc.host_address();
        if ip.is_unspecified() {
            return Err("The 3DS is not on Wi-Fi. Turn on wireless and connect to your network, then try again.".to_owned());
        }
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 80))
            .map_err(|err| format!("The page could not start on port 80: {err}"))?;
        listener.set_nonblocking(true).map_err(|err| format!("The page could not start: {err}"))?;
        // The page still works by IP address if mDNS cannot start.
        let mdns = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT))
            .and_then(|socket| {
                socket.join_multicast_v4(&MDNS_GROUP, &ip)?;
                socket.set_nonblocking(true)?;
                Ok(socket)
            })
            .map_err(|err| job::log(&format!("mDNS could not start: {err}")))
            .ok();
        job::log(&format!("Sharing at {ip}, mDNS {}", if mdns.is_some() { "on" } else { "off" }));
        Ok(Self { listener, mdns, connections: Vec::new(), ip, frames: 0, last_status: String::new(), _soc: soc })
    }

    /// The lines for the sharing page.
    pub fn address_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.mdns.is_some() {
            lines.push(format!("http://{}", net::HOST_NAME));
            lines.push(format!("or http://{}", self.ip));
        } else {
            lines.push(format!("http://{}", self.ip));
        }
        lines
    }

    fn announce(&self) {
        if let Some(socket) = &self.mdns {
            let packet = net::mdns_announcement(net::HOST_NAME, self.ip.octets());
            let _ = socket.send_to(&packet, SocketAddrV4::new(MDNS_GROUP, MDNS_PORT));
        }
    }

    /// The largest file that is arriving now, if any.
    pub fn upload(&self) -> Option<Upload> {
        self.connections
            .iter()
            .filter_map(|connection| connection.upload)
            .max_by_key(|&(_, _, total)| total)
            .map(|(kind, received, total)| Upload { kind, received, total })
    }

    /// Does one frame of work. Call it every frame while the sharing page shows.
    pub fn poll(&mut self, content: &Content) -> Vec<Event> {
        let mut events = Vec::new();
        if self.frames % ANNOUNCE_EVERY == 0 {
            self.announce();
        }
        self.frames = self.frames.wrapping_add(1);

        if let Some(socket) = &self.mdns {
            let mut packet = [0u8; 1500];
            while let Ok((len, from)) = socket.recv_from(&mut packet) {
                if let Some(answer) = net::mdns_answer(&packet[..len], net::HOST_NAME, self.ip.octets()) {
                    // A query from port 5353 wants the answer sent to the group.
                    let to = if from.port() == MDNS_PORT { std::net::SocketAddr::V4(SocketAddrV4::new(MDNS_GROUP, MDNS_PORT)) } else { from };
                    let _ = socket.send_to(&answer, to);
                }
            }
        }

        while let Ok((stream, from)) = self.listener.accept() {
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            if self.connections.len() >= MOST_CONNECTIONS {
                // Make room by dropping the connection that has waited longest without a request.
                let quietest = self
                    .connections
                    .iter()
                    .enumerate()
                    .filter(|(_, connection)| matches!(&connection.state, State::Reading(buffer) if buffer.is_empty()) || matches!(connection.state, State::Closing { .. }))
                    .max_by_key(|(_, connection)| connection.idle)
                    .map(|(index, _)| index);
                match quietest {
                    Some(index) => {
                        self.connections.remove(index);
                    }
                    None => {
                        job::log(&format!("Sharing: too many connections, turned away {from}"));
                        continue;
                    }
                }
            }
            set_receive_buffer(&stream);
            self.connections.push(Connection { stream, state: State::Reading(Vec::new()), idle: 0, opened: std::time::Instant::now(), upload: None });
        }

        let status = status_json(content);
        let status_changed = status != self.last_status;
        if status_changed {
            self.last_status = status.clone();
        }

        for connection in &mut self.connections {
            connection.idle += 1;
            if let State::Socket { out } = &mut connection.state {
                if status_changed {
                    out.extend(net::websocket_text(&status));
                }
            }
            step(connection, content, &status, &mut events);
        }
        // Drop finished connections, and any quiet for about 20 seconds.
        self.connections.retain(|connection| connection.idle < 1200 || matches!(connection.state, State::Socket { .. }));
        self.connections.retain(|connection| !matches!(connection.state, State::Closed));
        events
    }
}

/// Moves one connection along as far as it can go without waiting.
fn step(connection: &mut Connection, content: &Content, status: &str, events: &mut Vec<Event>) {
    loop {
        match &mut connection.state {
            State::Reading(buffer) => {
                let mut chunk = vec![0u8; CHUNK];
                let mut read = 0;
                let started = std::time::Instant::now();
                while read < MOST_READ_PER_FRAME {
                    match connection.stream.read(&mut chunk) {
                        Ok(0) => {
                            connection.state = finished();
                            return;
                        }
                        Ok(len) => {
                            connection.idle = 0;
                            buffer.extend_from_slice(&chunk[..len]);
                            read += len;
                        }
                        // Nothing yet. Idle connections wait for the next frame, but a request
                        // that has started arriving keeps being read for a while.
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            if buffer.is_empty() || started.elapsed() >= UPLOAD_TIME_PER_FRAME {
                                break;
                            }
                        }
                        Err(_) => {
                            connection.state = finished();
                            return;
                        }
                    }
                }
                let request = match net::parse_request(buffer) {
                    Ok(Some(request)) => request,
                    Ok(None) => return,
                    Err(()) => {
                        connection.state = finished();
                        return;
                    }
                };
                if request.content_length > MOST_UPLOAD {
                    connection.state = reply(net::response("413 Payload Too Large", "text/plain", b"That file is too large."));
                    continue;
                }
                let end = request.body_start + request.content_length;
                let kind = match request.path.as_str() {
                    "/patch" => Some("patch"),
                    "/apworld" => Some("apworld"),
                    _ => None,
                };
                connection.upload = kind
                    .filter(|_| request.method == "POST" && buffer.len() < end)
                    .map(|kind| (kind, buffer.len() - request.body_start, request.content_length));
                if buffer.len() < end {
                    // Grow once to the full size, instead of doubling past it.
                    buffer.reserve_exact(end - buffer.len());
                    return;
                }
                let State::Reading(buffer) = std::mem::replace(&mut connection.state, State::Reading(Vec::new())) else {
                    unreachable!()
                };
                if request.content_length > 64 * 1024 {
                    job::log(&format!(
                        "Sharing: received {} bytes in {:.1} s",
                        request.content_length,
                        connection.opened.elapsed().as_secs_f32()
                    ));
                }
                connection.state = route(&request, &buffer[request.body_start..end], content, status, events);
            }
            State::Writing { data, sent, file } => {
                if *sent >= data.len() {
                    match file {
                        Some(source) => {
                            let mut chunk = vec![0u8; CHUNK];
                            match source.read(&mut chunk) {
                                Ok(0) | Err(_) => {
                                    *file = None;
                                    let _ = connection.stream.flush();
                                    return;
                                }
                                Ok(len) => {
                                    chunk.truncate(len);
                                    *data = chunk;
                                    *sent = 0;
                                }
                            }
                        }
                        None => {
                            // All sent: shut our side, then wait for the browser to close.
                            let _ = connection.stream.flush();
                            let _ = connection.stream.shutdown(std::net::Shutdown::Write);
                            connection.state = State::Closing { frames: 0 };
                            continue;
                        }
                    }
                }
                match connection.stream.write(&data[*sent..]) {
                    Ok(len) => {
                        connection.idle = 0;
                        *sent += len;
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => return,
                    Err(_) => {
                        connection.state = finished();
                        return;
                    }
                }
            }
            State::Closing { frames } => {
                *frames += 1;
                let mut chunk = [0u8; 512];
                loop {
                    match connection.stream.read(&mut chunk) {
                        Ok(0) => {
                            connection.state = finished();
                            return;
                        }
                        Ok(_) => {}
                        Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            connection.state = finished();
                            return;
                        }
                    }
                }
                if *frames > CLOSE_WAIT_FRAMES {
                    connection.state = finished();
                }
                return;
            }
            State::Closed => return,
            State::Socket { out } => {
                connection.idle = 0;
                let mut chunk = [0u8; 512];
                match connection.stream.read(&mut chunk) {
                    Ok(0) => {
                        connection.state = finished();
                        return;
                    }
                    Ok(len) if net::websocket_closed(&chunk[..len]) => {
                        connection.state = finished();
                        return;
                    }
                    Ok(_) => {}
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => {
                        connection.state = finished();
                        return;
                    }
                }
                if !out.is_empty() {
                    match connection.stream.write(out) {
                        Ok(len) => {
                            out.drain(..len);
                        }
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {}
                        Err(_) => connection.state = finished(),
                    }
                }
                return;
            }
        }
    }
}

/// Asks for a larger receive buffer. The default still works if this fails.
fn set_receive_buffer(stream: &TcpStream) {
    use std::os::fd::AsRawFd;
    let size = RECEIVE_BUFFER;
    // SAFETY: the socket is open for the life of `stream`, and `size` outlives the call.
    let result = unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            (&size as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if result != 0 {
        job::log("Sharing: could not enlarge the receive buffer");
    }
}

fn reply(data: Vec<u8>) -> State {
    State::Writing { data, sent: 0, file: None }
}

fn finished() -> State {
    State::Closed
}

fn text(status: &str, message: &str) -> State {
    reply(net::response(status, "text/plain; charset=utf-8", message.as_bytes()))
}

fn route(request: &net::Request, body: &[u8], content: &Content, status: &str, events: &mut Vec<Event>) -> State {
    let path = request.path.split('?').next().unwrap_or("");
    job::log(&format!("Sharing: {} {path}", request.method));
    match (request.method.as_str(), path) {
        ("GET", "/ws") if request.wants_websocket() => match net::websocket_accept(request) {
            Some(mut out) => {
                out.extend(net::websocket_text(status));
                State::Socket { out }
            }
            None => text("400 Bad Request", "Bad WebSocket request."),
        },
        ("GET", "/") | ("GET", "/index.html") => reply(net::response("200 OK", "text/html; charset=utf-8", PAGE.as_bytes())),
        ("GET", "/status") => reply(net::response("200 OK", "application/json", status.as_bytes())),
        ("GET", "/options.yaml") => {
            let name = format!("attachment; filename=\"{}.yaml\"", content.player.replace(['"', '\\'], ""));
            let mut data = net::response_head("200 OK", "application/x-yaml; charset=utf-8", content.options_file.len() as u64, &[("Content-Disposition", &name)]);
            data.extend_from_slice(content.options_file.as_bytes());
            reply(data)
        }
        ("GET", "/albw_3ds.apworld") => match File::open(APWORLD).and_then(|file| Ok((file.metadata()?.len(), file))) {
            Ok((length, file)) => State::Writing {
                data: net::response_head(
                    "200 OK",
                    "application/octet-stream",
                    length,
                    &[("Content-Disposition", "attachment; filename=\"albw_3ds.apworld\"")],
                ),
                sent: 0,
                file: Some(file),
            },
            Err(_) => text("404 Not Found", "The apworld is not on this 3DS yet."),
        },
        ("POST", "/patch") => {
            let name = request.header("X-File-Name").map(net::percent_decode).and_then(|name| net::safe_file_name(&name));
            let Some(name) = name else {
                return text("400 Bad Request", "The file needs a name.");
            };
            if !name.to_ascii_lowercase().ends_with(&format!(".{}", ap_patch::EXTENSION)) {
                return text("400 Bad Request", "Only .apalbw3ds patch files can be sent.");
            }
            // Checked before saving, so a wrong file never reaches the list.
            if let Err(problem) = ap_patch::read(body) {
                return text("400 Bad Request", &problem);
            }
            match save_patch(&name, body) {
                Ok(()) => {
                    job::log(&format!("Received patch {name}, {} bytes", body.len()));
                    events.push(Event::Saved(name.clone()));
                    text("200 OK", &format!("Saved {name} on the 3DS. Open Patch Files there to apply it."))
                }
                Err(err) => text("500 Internal Server Error", &format!("The 3DS could not save the file: {err}")),
            }
        }
        ("POST", "/apworld") => {
            // Checked before saving, so the page only ever offers an apworld this app can patch for.
            match ap_patch::apworld_version(body) {
                Ok(version) if version == ap_patch::apworld_version_needed() => match save_apworld(body, &version) {
                    Ok(()) => {
                        job::log(&format!("Received apworld {version}, {} bytes", body.len()));
                        events.push(Event::Apworld(version.clone()));
                        text("200 OK", &format!("Saved apworld {version} on the 3DS. Hosts can download it from this page now."))
                    }
                    Err(err) => text("500 Internal Server Error", &format!("The 3DS could not save the file: {err}")),
                },
                Ok(version) => text(
                    "400 Bad Request",
                    &format!("That is apworld {version}. This app needs {}.", ap_patch::apworld_version_needed()),
                ),
                Err(problem) => text("400 Bad Request", &problem),
            }
        }
        _ => text("404 Not Found", "Nothing here."),
    }
}

fn save_apworld(body: &[u8], version: &str) -> std::io::Result<()> {
    let target = Path::new(APWORLD);
    if let Some(folder) = target.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = target.with_extension("part");
    fs::write(&temporary, body)?;
    let _ = fs::remove_file(target);
    fs::rename(&temporary, target)?;
    fs::write(APWORLD_VERSION, format!("{version}\n"))
}

fn save_patch(name: &str, body: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(job::AP_DIR)?;
    let target = Path::new(job::AP_DIR).join(name);
    let temporary = target.with_extension("part");
    fs::write(&temporary, body)?;
    let _ = fs::remove_file(&target);
    fs::rename(&temporary, &target)
}
