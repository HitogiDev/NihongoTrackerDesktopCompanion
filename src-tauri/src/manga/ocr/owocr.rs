//! owocr as the OCR engine: a program the user installs and starts on this
//! computer (AuroraWright/owocr, Python, GPL-3.0). The app neither ships nor
//! installs it and has none of its code: it only talks to it over a
//! WebSocket on `127.0.0.1`.
//!
//! The protocol, as owocr 1.26 implements it (`owocr/run.py`,
//! `WebsocketServerThread` and `OutputResult`; `owocr/ocr.py` for the result):
//!
//! - started with `read_from=websocket write_to=websocket output_format=json`,
//!   it serves `websocket_port` (7331 by default) on every interface;
//! - a binary message is an image (whatever Pillow opens). owocr answers that
//!   client `True` when it queued it, or `False` while it is paused;
//! - when the image is read, the result is sent to **every** client
//!   connected, with no request id: `json.dumps(asdict(OcrResult))`, or plain
//!   text with `output_format=text`. When the engine fails nothing is sent;
//! - with `output_format=json` owocr skips the engines that give no
//!   coordinates (Manga OCR without its segmenter, `mangaocr`): with only
//!   those it does not start.
//!
//! So there is a single request in flight at a time (the OCR thread runs one
//! job after another), a result only counts after our `True` and when its
//! image is the size of ours, anything else is dropped, and a request that
//! gets no answer in time drops the connection, so its late result cannot be
//! taken for the next one.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

pub const DEFAULT_PORT: u16 = 7331;
/// How long a page may take. owocr on the CPU (Manga OCR segmented, meikiocr)
/// takes a few seconds for a page; an online engine can take longer.
pub const PAGE_TIMEOUT: Duration = Duration::from_secs(60);
/// "Test connection" waits this long for the result of its small image.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// No real result comes near this: a page has a few dozen lines.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

/// What can go wrong talking to owocr, as the codes the UI translates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwocrError {
    /// Nothing listens on the port (owocr is not running, or on another port).
    Unavailable,
    /// owocr answered `False`: it is paused.
    Paused,
    /// The image was queued, but no result came in time.
    Timeout,
    /// A result came as plain text: `output_format` is not `json`.
    NoCoordinates,
    /// Something answered that is not owocr.
    BadReply,
}

impl OwocrError {
    pub fn code(self) -> &'static str {
        match self {
            OwocrError::Unavailable => "owocr-unavailable",
            OwocrError::Paused => "owocr-paused",
            OwocrError::Timeout => "owocr-timeout",
            OwocrError::NoCoordinates => "owocr-no-coordinates",
            OwocrError::BadReply => "owocr-bad-reply",
        }
    }

    /// Every other page would fail the same way: the queue is dropped and
    /// the error shown, instead of failing page after page.
    pub fn stops_queue(self) -> bool {
        !matches!(self, OwocrError::Timeout)
    }
}

impl std::fmt::Display for OwocrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

// ── owocr's result (`owocr/ocr.py`, `OcrResult` through `asdict`) ─────────

/// A box normalized to the image (0–1): centre, size and, for some engines,
/// a rotation in radians.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub struct BoundingBox {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub rotation_z: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Word {
    #[serde(default)]
    pub text: String,
    pub bounding_box: BoundingBox,
    #[serde(default)]
    pub separator: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Line {
    pub bounding_box: BoundingBox,
    #[serde(default)]
    pub words: Vec<Word>,
    /// The whole line, when the engine gives it.
    #[serde(default)]
    pub text: Option<String>,
    /// `LEFT_TO_RIGHT`, `RIGHT_TO_LEFT`, `TOP_TO_BOTTOM`, or nothing.
    #[serde(default)]
    pub writing_direction: Option<String>,
}

impl Line {
    /// Its text as owocr takes it (`TextFiltering.get_line_text`): the line's
    /// own, or the words joined by their separators (a space if none).
    pub fn joined_text(&self) -> String {
        if let Some(text) = &self.text {
            return text.clone();
        }
        let mut out = String::new();
        for (i, word) in self.words.iter().enumerate() {
            out.push_str(&word.text);
            if i + 1 < self.words.len() {
                out.push_str(word.separator.as_deref().unwrap_or(" "));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Paragraph {
    pub bounding_box: BoundingBox,
    #[serde(default)]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub writing_direction: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct ImageProperties {
    pub width: u32,
    pub height: u32,
}

/// Which parts the engine really gives (the rest owocr fills in).
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct EngineCapabilities {
    pub lines: bool,
    pub line_bounding_boxes: bool,
    pub paragraphs: bool,
    pub paragraph_bounding_boxes: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OcrResult {
    pub image_properties: ImageProperties,
    #[serde(default)]
    pub engine_capabilities: Option<EngineCapabilities>,
    pub paragraphs: Vec<Paragraph>,
}

/// What a message after our `True` is.
#[derive(Debug, PartialEq)]
enum Reply {
    Result(OcrResult),
    /// Not JSON at all: owocr writes plain text.
    Text,
    /// JSON, but not a result.
    Other,
}

fn parse_reply(text: &str) -> Reply {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') {
        return Reply::Text;
    }
    match serde_json::from_str::<OcrResult>(trimmed) {
        Ok(result) => Reply::Result(result),
        Err(_) if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() => Reply::Other,
        // Text that happens to start with a brace.
        Err(_) => Reply::Text,
    }
}

// ── from owocr's boxes to `.mokuro` blocks ────────────────────────────

/// A paragraph per block, at most as many as a page corrected by hand.
const MAX_BLOCKS: usize = 400;

fn finite(b: &BoundingBox) -> bool {
    [b.center_x, b.center_y, b.width, b.height, b.rotation_z.unwrap_or(0.0)].iter().all(|v| v.is_finite()) && b.width >= 0.0 && b.height >= 0.0
}

/// The four corners of a box in image pixels, clockwise from the top left
/// before rotating (the order `.mokuro` gives a line's corners). owocr makes
/// a rotated box from a quadrilateral in pixels (`quad_to_bounding_box`: the
/// sides' length over the image's width and height, the angle of the top
/// side), so it is rotated back in pixels too.
pub fn corners(b: &BoundingBox, image: (f64, f64)) -> [[f64; 2]; 4] {
    let (w, h) = image;
    let (cx, cy) = (b.center_x * w, b.center_y * h);
    let (hw, hh) = (b.width * w / 2.0, b.height * h / 2.0);
    let angle = b.rotation_z.unwrap_or(0.0);
    let (sin, cos) = angle.sin_cos();
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map(|[x, y]| [cx + x * cos - y * sin, cy + x * sin + y * cos])
}

/// The upright box around some points, inside the image.
fn hull(points: impl IntoIterator<Item = [f64; 2]>, image: (f64, f64)) -> Option<[f64; 4]> {
    let mut out: Option<[f64; 4]> = None;
    for [x, y] in points {
        let b = out.get_or_insert([x, y, x, y]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    out.map(|b| [b[0].clamp(0.0, image.0), b[1].clamp(0.0, image.1), b[2].clamp(0.0, image.0), b[3].clamp(0.0, image.1)])
}

/// `TOP_TO_BOTTOM` is vertical; any other direction given is not.
fn vertical_from(direction: Option<&str>) -> Option<bool> {
    direction.filter(|d| !d.is_empty()).map(|d| d.eq_ignore_ascii_case("TOP_TO_BOTTOM"))
}

fn has_japanese(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}' | '\u{3000}'..='\u{303f}'))
}

/// A line's text the way manga-ocr leaves it (what owocr's own text output
/// does to Japanese lines); other lines only lose extra spaces.
fn clean(text: &str) -> String {
    if has_japanese(text) {
        super::text::post_process(text)
    } else {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

struct PlacedLine {
    text: String,
    corners: [[f64; 2]; 4],
    /// Upright box around it.
    hull: [f64; 4],
    /// Its thickness in pixels: the shorter side of its own box.
    thickness: f64,
    vertical: Option<bool>,
}

fn block(paragraph: &Paragraph, image: (f64, f64)) -> Option<super::super::mokuro::MokuroBlock> {
    let mut lines: Vec<PlacedLine> = paragraph
        .lines
        .iter()
        .filter(|line| finite(&line.bounding_box))
        .filter_map(|line| {
            let text = clean(&line.joined_text());
            if text.is_empty() {
                return None;
            }
            let corners = corners(&line.bounding_box, image);
            let hull = hull(corners, image)?;
            let b = &line.bounding_box;
            let thickness = (b.width * image.0).min(b.height * image.1);
            Some(PlacedLine { text, corners, hull, thickness, vertical: vertical_from(line.writing_direction.as_deref()) })
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    let own = finite(&paragraph.bounding_box).then(|| corners(&paragraph.bounding_box, image));
    let bbox = hull(own.into_iter().flatten().chain(lines.iter().flat_map(|line| line.corners)), image)?;
    if bbox[2] - bbox[0] < 1.0 || bbox[3] - bbox[1] < 1.0 {
        return None;
    }
    // Which way it is written: as the paragraph says, or its lines, or, if
    // nobody says, the shape of its lines (as the built-in OCR guesses).
    let said: Vec<bool> = lines.iter().filter_map(|line| line.vertical).collect();
    let vertical = vertical_from(paragraph.writing_direction.as_deref())
        .or_else(|| (!said.is_empty()).then(|| said.iter().filter(|v| **v).count() * 2 > said.len()))
        .unwrap_or_else(|| {
            let tall = lines.iter().filter(|line| super::layout::is_vertical(&line.hull)).count();
            if lines.len() == 1 { super::layout::is_vertical(&bbox) || tall == 1 } else { tall * 2 > lines.len() }
        });
    // Columns from the right; rows from the top.
    if vertical {
        lines.sort_by(|a, b| (b.hull[0] + b.hull[2]).total_cmp(&(a.hull[0] + a.hull[2])));
    } else {
        lines.sort_by(|a, b| (a.hull[1] + a.hull[3]).total_cmp(&(b.hull[1] + b.hull[3])).then((a.hull[0]).total_cmp(&b.hull[0])));
    }
    let mut sizes: Vec<f64> = lines.iter().map(|line| line.thickness).filter(|v| *v > 0.0).collect();
    sizes.sort_by(f64::total_cmp);
    let font_size = sizes.get(sizes.len() / 2).copied().unwrap_or(16.0).round().max(1.0);
    Some(super::super::mokuro::MokuroBlock {
        bbox,
        vertical,
        font_size,
        lines_coords: lines.iter().map(|line| line.corners.to_vec()).collect(),
        lines: lines.into_iter().map(|line| line.text).collect(),
    })
}

/// owocr's result for a page as a `.mokuro` page: a block per paragraph,
/// its lines (columns, when vertical) with their own corners, in the order
/// a manga is read (the built-in OCR's `layout::reading_order`).
pub fn to_page(result: &OcrResult, img_path: &str) -> super::super::mokuro::MokuroPage {
    let image = (f64::from(result.image_properties.width), f64::from(result.image_properties.height));
    let blocks: Vec<_> = result.paragraphs.iter().filter_map(|paragraph| block(paragraph, image)).take(MAX_BLOCKS).collect();
    let boxes: Vec<[f64; 4]> = blocks.iter().map(|block| block.bbox).collect();
    let mut slots: Vec<Option<_>> = blocks.into_iter().map(Some).collect();
    let blocks = super::layout::reading_order(&boxes).into_iter().filter_map(|index| slots[index].take()).collect();
    super::super::mokuro::MokuroPage { img_path: img_path.to_string(), img_width: image.0, img_height: image.1, blocks }
}

/// The text inside a box the user drew, sent with a margin around it: the
/// lines whose centre is inside `inside` (in the image's pixels), in reading
/// order, as one string, like the built-in OCR gives it.
pub fn to_text(result: &OcrResult, inside: [f64; 4]) -> String {
    let within = |corners: &Vec<[f64; 2]>| {
        let n = corners.len().max(1) as f64;
        let (x, y) = corners.iter().fold((0.0, 0.0), |(x, y), c| (x + c[0] / n, y + c[1] / n));
        (inside[0]..=inside[2]).contains(&x) && (inside[1]..=inside[3]).contains(&y)
    };
    to_page(result, "")
        .blocks
        .iter()
        .flat_map(|block| block.lines.iter().zip(&block.lines_coords))
        .filter(|(_, corners)| within(corners))
        .map(|(line, _)| line.as_str())
        .collect()
}

// ── the client ────────────────────────────────────────────────────────

type Socket = WebSocket<TcpStream>;

/// The connection to owocr, kept open between pages. Used from the OCR
/// thread only, one request at a time.
pub struct Client {
    port: u16,
    timeout: Duration,
    socket: Option<Socket>,
}

fn is_timeout(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
}

/// Always this computer: owocr listens on every interface, but pages are
/// never sent to another machine.
fn address(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn connect(port: u16) -> Result<Socket, OwocrError> {
    let stream = TcpStream::connect_timeout(&address(port), CONNECT_TIMEOUT).map_err(|_| OwocrError::Unavailable)?;
    let _ = stream.set_nodelay(true);
    stream.set_read_timeout(Some(CONNECT_TIMEOUT)).map_err(|_| OwocrError::Unavailable)?;
    stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|_| OwocrError::Unavailable)?;
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(MAX_MESSAGE);
    config.max_frame_size = Some(MAX_MESSAGE);
    let url = format!("ws://127.0.0.1:{port}/");
    match tungstenite::client::client_with_config(url.as_str(), stream, Some(config)) {
        Ok((socket, _)) => Ok(socket),
        // Something listens, but it is not a WebSocket server.
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(_) | tungstenite::Error::Protocol(_))) => Err(OwocrError::BadReply),
        Err(_) => Err(OwocrError::Unavailable),
    }
}

impl Client {
    pub fn new(port: u16, timeout: Duration) -> Self {
        Client { port, timeout, socket: None }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Throws away what arrived since the last request: results of other
    /// clients' images (owocr sends every result to everyone).
    fn drain(socket: &mut Socket) -> Result<(), OwocrError> {
        socket.get_mut().set_nonblocking(true).map_err(|_| OwocrError::Unavailable)?;
        let outcome = loop {
            match socket.read() {
                Ok(_) => continue,
                Err(error) if is_timeout(&error) => break Ok(()),
                Err(_) => break Err(OwocrError::Unavailable),
            }
        };
        socket.get_mut().set_nonblocking(false).map_err(|_| OwocrError::Unavailable)?;
        outcome
    }

    /// The open connection, with nothing stale in it; a new one when there
    /// is none or it closed (owocr was restarted).
    fn ready_socket(&mut self) -> Result<&mut Socket, OwocrError> {
        if let Some(socket) = self.socket.as_mut() {
            if Self::drain(socket).is_err() {
                self.socket = None;
            }
        }
        if self.socket.is_none() {
            self.socket = Some(connect(self.port)?);
        }
        Ok(self.socket.as_mut().expect("connected above"))
    }

    /// Sends one image (any format Pillow opens) of `size` pixels and waits
    /// for its result.
    pub fn recognize(&mut self, image: &[u8], size: (u32, u32)) -> Result<OcrResult, OwocrError> {
        let outcome = self.exchange(image, size);
        if outcome.is_err() {
            // A late result must not land on the next request.
            self.socket = None;
        }
        outcome
    }

    fn exchange(&mut self, image: &[u8], size: (u32, u32)) -> Result<OcrResult, OwocrError> {
        let timeout = self.timeout;
        let mut socket = self.ready_socket()?;
        if socket.send(Message::Binary(image.to_vec())).is_err() {
            // Closed while idle: once more on a new connection.
            self.socket = None;
            socket = self.ready_socket()?;
            socket.send(Message::Binary(image.to_vec())).map_err(|_| OwocrError::Unavailable)?;
        }
        let deadline = Instant::now() + timeout;
        let mut accepted = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(OwocrError::Timeout);
            }
            socket.get_mut().set_read_timeout(Some(left)).map_err(|_| OwocrError::Unavailable)?;
            let message = match socket.read() {
                Ok(message) => message,
                Err(error) if is_timeout(&error) => return Err(OwocrError::Timeout),
                Err(tungstenite::Error::Capacity(_)) => return Err(OwocrError::BadReply),
                Err(_) => return Err(OwocrError::Unavailable),
            };
            let Message::Text(text) = message else { continue };
            if !accepted {
                // Our image's answer comes first; a result before it is
                // somebody else's.
                match text.as_str() {
                    "True" => accepted = true,
                    "False" => return Err(OwocrError::Paused),
                    _ => log::debug!("owocr: dropped a message before the answer to ours"),
                }
                continue;
            }
            match parse_reply(&text) {
                Reply::Result(result) if (result.image_properties.width, result.image_properties.height) == size => return Ok(result),
                Reply::Result(result) => {
                    log::debug!("owocr: dropped the result of a {}×{} image", result.image_properties.width, result.image_properties.height);
                }
                Reply::Text => return Err(OwocrError::NoCoordinates),
                Reply::Other => return Err(OwocrError::BadReply),
            }
        }
    }
}

// ── "Test connection" ─────────────────────────────────────────────────

/// What "Test connection" found.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct OwocrProbe {
    /// `ws://127.0.0.1:<port>`.
    pub url: String,
    /// It answered with a result that has coordinates.
    pub ok: bool,
    /// `ok`, or an error code (`owocr-unavailable`, `owocr-paused`,
    /// `owocr-timeout`, `owocr-no-coordinates`, `owocr-bad-reply`).
    pub code: String,
    /// The engine gives boxes per line (not only made-up ones).
    pub line_boxes: bool,
    /// Milliseconds from sending the image to its result.
    pub millis: u64,
}

/// A small blank image: enough to see what owocr answers.
pub fn probe_image() -> (Vec<u8>, (u32, u32)) {
    let size = (160u32, 64u32);
    let image = image::RgbImage::from_pixel(size.0, size.1, image::Rgb([255, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).expect("encoding a PNG in memory");
    (png.into_inner(), size)
}

/// Connects to owocr on `port` (always this computer), sends a small image
/// and says what came back.
pub fn probe(port: u16, timeout: Duration) -> OwocrProbe {
    let (image, size) = probe_image();
    let started = Instant::now();
    let outcome = Client::new(port, timeout).recognize(&image, size);
    let millis = started.elapsed().as_millis() as u64;
    let url = format!("ws://127.0.0.1:{port}");
    match outcome {
        Ok(result) => OwocrProbe {
            url,
            ok: true,
            code: "ok".into(),
            // Older owocr sends no capabilities: its lines have boxes.
            line_boxes: result.engine_capabilities.map_or(true, |caps| caps.line_bounding_boxes),
            millis,
        },
        Err(error) => OwocrProbe { url, ok: false, code: error.code().into(), line_boxes: false, millis },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    /// How the fake owocr answers an image.
    #[derive(Clone, Copy, PartialEq)]
    pub enum Behaviour {
        /// `True`, then the result to every client.
        Answer,
        /// `True`, then somebody else's result (another size) and then ours.
        StrayFirst,
        /// `True`, then a result for the right size only after `ms`.
        Late(u64),
        /// `False`: paused.
        Paused,
        /// `True` and nothing else (the engine failed).
        Silent,
        /// `True` and the text, as with `output_format=text`.
        Text,
    }

    /// A result for an image of `size`, with one vertical paragraph.
    pub fn result_json(size: (u32, u32)) -> String {
        format!(
            r#"{{"image_properties": {{"width": {}, "height": {}, "x": null, "y": null, "window_handle": null, "window_x": null, "window_y": null}},
               "engine_capabilities": {{"symbols": false, "symbol_bounding_boxes": false, "words": false, "word_bounding_boxes": false, "lines": true, "line_bounding_boxes": true, "paragraphs": true, "paragraph_bounding_boxes": true}},
               "paragraphs": [{{"bounding_box": {{"center_x": 0.5, "center_y": 0.5, "width": 0.2, "height": 0.6, "rotation_z": null}},
                 "lines": [{{"bounding_box": {{"center_x": 0.5, "center_y": 0.5, "width": 0.1, "height": 0.6, "rotation_z": null}}, "words": [], "text": "テスト", "writing_direction": null}}],
                 "writing_direction": "TOP_TO_BOTTOM"}}]}}"#,
            size.0, size.1
        )
    }

    /// A fake owocr on a free port of 127.0.0.1, with the same protocol:
    /// `True`/`False` to the sender, results to every client.
    pub struct FakeOwocr {
        pub port: u16,
        /// Images received.
        pub received: Arc<Mutex<usize>>,
    }

    impl FakeOwocr {
        pub fn start(behaviour: Behaviour) -> FakeOwocr {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let received = Arc::new(Mutex::new(0usize));
            let clients: Arc<Mutex<Vec<mpsc::Sender<String>>>> = Arc::default();
            let count = received.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let clients = clients.clone();
                    let count = count.clone();
                    std::thread::spawn(move || serve(stream, behaviour, clients, count));
                }
            });
            FakeOwocr { port, received }
        }
    }

    fn serve(stream: TcpStream, behaviour: Behaviour, clients: Arc<Mutex<Vec<mpsc::Sender<String>>>>, count: Arc<Mutex<usize>>) {
        let Ok(mut socket) = tungstenite::accept(stream) else { return };
        let (outbox, inbox) = mpsc::channel::<String>();
        clients.lock().unwrap().push(outbox);
        let broadcast = |text: String| {
            for client in clients.lock().unwrap().iter() {
                let _ = client.send(text.clone());
            }
        };
        socket.get_mut().set_read_timeout(Some(Duration::from_millis(20))).unwrap();
        loop {
            while let Ok(text) = inbox.try_recv() {
                if socket.send(Message::Text(text)).is_err() {
                    return;
                }
            }
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    *count.lock().unwrap() += 1;
                    let size = image::load_from_memory(&bytes).map(|image| (image.width(), image.height())).unwrap_or((0, 0));
                    if behaviour == Behaviour::Paused {
                        let _ = socket.send(Message::Text("False".into()));
                        continue;
                    }
                    let _ = socket.send(Message::Text("True".into()));
                    match behaviour {
                        Behaviour::Answer => broadcast(result_json(size)),
                        Behaviour::StrayFirst => {
                            broadcast(result_json((size.0 + 1, size.1)));
                            broadcast(result_json(size));
                        }
                        Behaviour::Late(ms) => {
                            let clients = clients.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(ms));
                                for client in clients.lock().unwrap().iter() {
                                    let _ = client.send(result_json(size));
                                }
                            });
                        }
                        Behaviour::Text => broadcast("テスト".into()),
                        Behaviour::Silent | Behaviour::Paused => {}
                    }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(error) if is_timeout(&error) => {}
                Err(_) => return,
            }
        }
    }

    fn page(size: (u32, u32)) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(size.0, size.1, image::Rgb([250, 250, 250]));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    #[test]
    fn the_result_of_our_image_comes_back() {
        let owocr = FakeOwocr::start(Behaviour::Answer);
        let mut client = Client::new(owocr.port, Duration::from_secs(5));
        let result = client.recognize(&page((120, 300)), (120, 300)).unwrap();
        assert_eq!(result.image_properties, ImageProperties { width: 120, height: 300 });
        assert_eq!(result.paragraphs[0].lines[0].joined_text(), "テスト");
        // The connection is kept for the next page.
        let again = client.recognize(&page((90, 40)), (90, 40)).unwrap();
        assert_eq!(again.image_properties.width, 90);
        assert_eq!(*owocr.received.lock().unwrap(), 2);
    }

    #[test]
    fn a_result_for_another_image_is_dropped() {
        let owocr = FakeOwocr::start(Behaviour::StrayFirst);
        let result = Client::new(owocr.port, Duration::from_secs(5)).recognize(&page((200, 100)), (200, 100)).unwrap();
        assert_eq!(result.image_properties, ImageProperties { width: 200, height: 100 });
    }

    #[test]
    fn what_other_clients_asked_for_is_not_taken_for_ours() {
        let owocr = FakeOwocr::start(Behaviour::Answer);
        let mut ours = Client::new(owocr.port, Duration::from_secs(5));
        // Connected, idle.
        ours.recognize(&page((10, 10)), (10, 10)).unwrap();
        // Another client asks for an image the size of ours: the result is
        // sent to everyone, ours included.
        Client::new(owocr.port, Duration::from_secs(5)).recognize(&page((64, 64)), (64, 64)).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        // Our next request does not take it: it was waiting before our
        // `True`. owocr only got our image after that, and answers it.
        let before = *owocr.received.lock().unwrap();
        ours.recognize(&page((64, 64)), (64, 64)).unwrap();
        assert_eq!(*owocr.received.lock().unwrap(), before + 1);
    }

    #[test]
    fn paused_owocr_says_so() {
        let owocr = FakeOwocr::start(Behaviour::Paused);
        let error = Client::new(owocr.port, Duration::from_secs(5)).recognize(&page((10, 10)), (10, 10)).unwrap_err();
        assert_eq!(error, OwocrError::Paused);
        assert!(error.stops_queue());
    }

    #[test]
    fn no_result_in_time_is_a_timeout_and_the_late_one_is_not_reused() {
        let owocr = FakeOwocr::start(Behaviour::Late(600));
        let mut client = Client::new(owocr.port, Duration::from_millis(300));
        let started = Instant::now();
        assert_eq!(client.recognize(&page((30, 30)), (30, 30)).unwrap_err(), OwocrError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!OwocrError::Timeout.stops_queue());
        // The late result of the first page goes to the dropped connection;
        // the next page, the same size, waits for its own.
        std::thread::sleep(Duration::from_millis(400));
        let mut patient = Client::new(owocr.port, Duration::from_secs(3));
        assert!(patient.recognize(&page((30, 30)), (30, 30)).is_ok());

        let silent = FakeOwocr::start(Behaviour::Silent);
        assert_eq!(Client::new(silent.port, Duration::from_millis(200)).recognize(&page((5, 5)), (5, 5)).unwrap_err(), OwocrError::Timeout);
    }

    #[test]
    fn owocr_not_running_is_unavailable() {
        let port = free_port();
        let started = Instant::now();
        assert_eq!(Client::new(port, Duration::from_secs(5)).recognize(&page((5, 5)), (5, 5)).unwrap_err(), OwocrError::Unavailable);
        assert!(started.elapsed() < Duration::from_secs(3));
        let probe = probe(port, Duration::from_secs(1));
        assert!(!probe.ok);
        assert_eq!(probe.code, "owocr-unavailable");
        assert_eq!(probe.url, format!("ws://127.0.0.1:{port}"));
    }

    #[test]
    fn something_else_on_the_port_is_not_owocr() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
            }
        });
        assert_eq!(Client::new(port, Duration::from_secs(2)).recognize(&page((5, 5)), (5, 5)).unwrap_err(), OwocrError::BadReply);
    }

    #[test]
    fn plain_text_means_no_coordinates() {
        let owocr = FakeOwocr::start(Behaviour::Text);
        let probe = probe(owocr.port, Duration::from_secs(5));
        assert!(!probe.ok);
        assert_eq!(probe.code, "owocr-no-coordinates");
    }

    #[test]
    fn the_probe_reports_a_working_owocr() {
        let owocr = FakeOwocr::start(Behaviour::Answer);
        let probe = probe(owocr.port, Duration::from_secs(5));
        assert!(probe.ok, "{probe:?}");
        assert_eq!(probe.code, "ok");
        assert!(probe.line_boxes);
    }

    #[test]
    fn replies_are_told_apart() {
        assert!(matches!(parse_reply(&result_json((3, 4))), Reply::Result(_)));
        assert_eq!(parse_reply("こんにちは"), Reply::Text);
        assert_eq!(parse_reply("{笑}"), Reply::Text);
        assert_eq!(parse_reply(r#"{"type": "hello"}"#), Reply::Other);
        // owocr leaves out nothing, but an older one may: tolerated.
        let minimal = r#"{"image_properties": {"width": 3, "height": 4}, "paragraphs": []}"#;
        assert!(matches!(parse_reply(minimal), Reply::Result(r) if r.engine_capabilities.is_none()));
    }

    #[test]
    fn words_are_joined_like_owocr_does() {
        let bbox = BoundingBox { center_x: 0.5, center_y: 0.5, width: 0.1, height: 0.1, rotation_z: None };
        let word = |text: &str, separator: Option<&str>| Word { text: text.into(), bounding_box: bbox, separator: separator.map(str::to_string) };
        let line = Line { bounding_box: bbox, words: vec![word("今日", Some("")), word("は", None), word("晴れ", None)], text: None, writing_direction: None };
        assert_eq!(line.joined_text(), "今日は 晴れ");
    }

    // ── conversion ────────────────────────────────────────────────────

    fn bbox(cx: f64, cy: f64, w: f64, h: f64, rotation: Option<f64>) -> BoundingBox {
        BoundingBox { center_x: cx, center_y: cy, width: w, height: h, rotation_z: rotation }
    }

    /// Written by hand with the shape owocr 1.26 sends (`asdict(OcrResult)`):
    /// a 1000×1500 page with a vertical bubble on the right (two columns,
    /// given left to right, words without the line's text), a caption at
    /// the bottom, slightly rotated, and a bubble on the left with no
    /// direction at all.
    const PAGE: &str = r#"{
      "image_properties": {"width": 1000, "height": 1500, "x": null, "y": null, "window_handle": null, "window_x": null, "window_y": null},
      "engine_capabilities": {"symbols": false, "symbol_bounding_boxes": false, "words": true, "word_bounding_boxes": true, "lines": true, "line_bounding_boxes": true, "paragraphs": true, "paragraph_bounding_boxes": true},
      "paragraphs": [
        {"bounding_box": {"center_x": 0.2, "center_y": 0.2, "width": 0.06, "height": 0.2, "rotation_z": null},
         "lines": [{"bounding_box": {"center_x": 0.2, "center_y": 0.2, "width": 0.05, "height": 0.2, "rotation_z": null}, "words": [], "text": "左の吹き出し", "writing_direction": null}],
         "writing_direction": null},
        {"bounding_box": {"center_x": 0.5, "center_y": 0.9, "width": 0.4, "height": 0.04, "rotation_z": 0.1},
         "lines": [{"bounding_box": {"center_x": 0.5, "center_y": 0.9, "width": 0.4, "height": 0.04, "rotation_z": 0.1},
                    "words": [{"text": "東京駅", "bounding_box": {"center_x": 0.4, "center_y": 0.9, "width": 0.15, "height": 0.04, "rotation_z": 0.1}, "separator": "", "symbols": null},
                              {"text": "で...", "bounding_box": {"center_x": 0.6, "center_y": 0.9, "width": 0.15, "height": 0.04, "rotation_z": 0.1}, "separator": null, "symbols": null}],
                    "text": null, "writing_direction": "LEFT_TO_RIGHT"}],
         "writing_direction": "LEFT_TO_RIGHT"},
        {"bounding_box": {"center_x": 0.8, "center_y": 0.2, "width": 0.1, "height": 0.2, "rotation_z": null},
         "lines": [
           {"bounding_box": {"center_x": 0.775, "center_y": 0.2, "width": 0.04, "height": 0.18, "rotation_z": null}, "words": [{"text": "散歩", "bounding_box": {"center_x": 0.775, "center_y": 0.15, "width": 0.04, "height": 0.05, "rotation_z": null}, "separator": null, "symbols": null}, {"text": "しよう", "bounding_box": {"center_x": 0.775, "center_y": 0.23, "width": 0.04, "height": 0.1, "rotation_z": null}, "separator": "", "symbols": null}], "text": null, "writing_direction": "TOP_TO_BOTTOM"},
           {"bounding_box": {"center_x": 0.825, "center_y": 0.2, "width": 0.04, "height": 0.2, "rotation_z": null}, "words": [], "text": "そうだね", "writing_direction": "TOP_TO_BOTTOM"},
           {"bounding_box": {"center_x": 0.7, "center_y": 0.2, "width": 0.04, "height": 0.2, "rotation_z": null}, "words": [], "text": "   ", "writing_direction": "TOP_TO_BOTTOM"}
         ],
         "writing_direction": "TOP_TO_BOTTOM"}
      ]
    }"#;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn a_page_from_owocr_becomes_mokuro_blocks_in_reading_order() {
        let Reply::Result(result) = parse_reply(PAGE) else { panic!("not a result") };
        let page = to_page(&result, "007.jpg");
        assert_eq!(page.img_path, "007.jpg");
        assert_eq!((page.img_width, page.img_height), (1000.0, 1500.0));
        // Right bubble, left bubble (same row, right to left), then the caption.
        let texts: Vec<Vec<String>> = page.blocks.iter().map(|block| block.lines.clone()).collect();
        assert_eq!(texts, [vec!["そうだね".to_string(), "散歩しよう".to_string()], vec!["左の吹き出し".to_string()], vec!["東京駅で．．．".to_string()]]);

        let right = &page.blocks[0];
        assert!(right.vertical);
        // The paragraph's own box: 0.75–0.85 × 0.1–0.3 of the page.
        assert!(close(right.bbox[0], 750.0) && close(right.bbox[1], 150.0) && close(right.bbox[2], 850.0) && close(right.bbox[3], 450.0), "{:?}", right.bbox);
        // Columns from the right, each with its own corners.
        assert!(right.lines_coords[0][0][0] > right.lines_coords[1][0][0]);
        assert_eq!(right.lines_coords[0], vec![[805.0, 150.0], [845.0, 150.0], [845.0, 450.0], [805.0, 450.0]]);
        assert_eq!(right.font_size, 40.0);

        // No direction given: a tall line reads as a column.
        assert!(page.blocks[1].vertical);
    }

    #[test]
    fn rotated_boxes_keep_their_corners() {
        let Reply::Result(result) = parse_reply(PAGE) else { panic!("not a result") };
        let caption = to_page(&result, "007.jpg").blocks.pop().unwrap();
        assert!(!caption.vertical);
        // 400 × 60 px around (500, 1350), turned 0.1 rad clockwise on screen.
        let corners = &caption.lines_coords[0];
        let (sin, cos) = 0.1f64.sin_cos();
        assert!(close(corners[0][0], 500.0 - 200.0 * cos + 30.0 * sin) && close(corners[0][1], 1350.0 - 200.0 * sin - 30.0 * cos), "{corners:?}");
        assert!(close(corners[2][0], 500.0 + 200.0 * cos - 30.0 * sin) && close(corners[2][1], 1350.0 + 200.0 * sin + 30.0 * cos));
        // The block is the upright box around them: taller than the line.
        assert!(caption.bbox[3] - caption.bbox[1] > 60.0 + 30.0);
        assert!(close(caption.bbox[0], corners[3][0].min(corners[0][0])));
        assert_eq!(caption.font_size, 60.0);
    }

    #[test]
    fn a_horizontal_paragraph_reads_its_lines_top_down() {
        let line = |cy: f64, text: &str| Line { bounding_box: bbox(0.5, cy, 0.6, 0.02, None), words: vec![], text: Some(text.into()), writing_direction: None };
        let result = OcrResult {
            image_properties: ImageProperties { width: 800, height: 1000 },
            engine_capabilities: None,
            paragraphs: vec![Paragraph {
                bounding_box: bbox(0.5, 0.5, 0.6, 0.1, None),
                lines: vec![line(0.53, "二行目"), line(0.47, "一行目")],
                writing_direction: Some("LEFT_TO_RIGHT".into()),
            }],
        };
        let page = to_page(&result, "p.png");
        assert_eq!(page.blocks[0].lines, ["一行目", "二行目"]);
        assert!(!page.blocks[0].vertical);
        assert_eq!(to_text(&result, [0.0, 0.0, 800.0, 1000.0]), "一行目二行目");
        // A box drawn around the first line only (sent with a margin).
        assert_eq!(to_text(&result, [100.0, 450.0, 700.0, 480.0]), "一行目");
    }

    #[test]
    fn broken_or_empty_boxes_are_left_out() {
        let result = OcrResult {
            image_properties: ImageProperties { width: 100, height: 100 },
            engine_capabilities: None,
            paragraphs: vec![
                Paragraph { bounding_box: bbox(0.5, 0.5, 0.2, 0.2, None), lines: vec![], writing_direction: None },
                Paragraph {
                    bounding_box: bbox(f64::NAN, 0.5, 0.2, 0.2, None),
                    lines: vec![
                        Line { bounding_box: bbox(0.5, 0.5, f64::INFINITY, 0.2, None), words: vec![], text: Some("あ".into()), writing_direction: None },
                        // Past the edge: kept, inside the image.
                        Line { bounding_box: bbox(0.95, 0.5, 0.2, 0.3, None), words: vec![], text: Some("ｶﾀｶﾅ".into()), writing_direction: Some("TOP_TO_BOTTOM".into()) },
                    ],
                    writing_direction: None,
                },
            ],
        };
        let page = to_page(&result, "p.png");
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].lines, ["カタカナ"]);
        assert!(page.blocks[0].vertical);
        assert_eq!(page.blocks[0].bbox[2], 100.0);
    }

    /// Against a real owocr started by hand, e.g.
    /// `owocr -r websocket -w websocket -of json -e meikiocr`:
    /// `NTDC_OWOCR_PORT` (7331 if unset), `NTDC_OCR_PAGE` the page to read,
    /// `NTDC_OCR_EXPECT` (optional) its blocks' text joined by `|`.
    #[test]
    #[ignore]
    fn reads_a_real_page_with_owocr() {
        let port = std::env::var("NTDC_OWOCR_PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(DEFAULT_PORT);
        let probe = probe(port, PROBE_TIMEOUT);
        println!("{probe:?}");
        assert!(probe.ok);
        let path = std::env::var("NTDC_OCR_PAGE").expect("NTDC_OCR_PAGE");
        let bytes = std::fs::read(&path).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        let started = Instant::now();
        let result = Client::new(port, PAGE_TIMEOUT).recognize(&bytes, (image.width(), image.height())).unwrap();
        let page = to_page(&result, "page");
        println!("page {:?}", started.elapsed());
        for block in &page.blocks {
            println!("{:?} vertical={} lines={:?}", block.bbox.map(f64::round), block.vertical, block.lines);
        }
        assert!(!page.blocks.is_empty());
        // A box drawn around the first block reads the same text.
        let crop = super::super::crop_png(&image, page.blocks[0].bbox).unwrap();
        let region = to_text(&Client::new(port, PAGE_TIMEOUT).recognize(&crop.png, crop.size).unwrap(), crop.inside);
        println!("region {region:?}");
        assert_eq!(region, page.blocks[0].lines.concat());
        if let Ok(expected) = std::env::var("NTDC_OCR_EXPECT") {
            let read: Vec<String> = page.blocks.iter().map(|block| block.lines.concat()).collect();
            assert_eq!(read.join("|"), expected);
        }
    }
}
