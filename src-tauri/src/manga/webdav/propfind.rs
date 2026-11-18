//! `PROPFIND Depth: 1` answers (RFC 4918 `multistatus`), parsed into the
//! entries of one folder.
//!
//! Servers spell the same answer many ways: `D:`, `d:`, `lp1:` or no prefix at
//! all, `href`s as absolute URLs or as paths, percent-encoded or not, the
//! folder itself listed first or not. Elements are matched by local name, only
//! the properties of a `200` `propstat` are taken, and only the direct
//! children of the folder that was asked for come out: whatever else a server
//! puts in its answer is ignored.

use quick_xml::events::Event;
use quick_xml::NsReader;

/// An answer past this size is not a manga folder's listing.
pub const MAX_LISTING_BYTES: usize = 32 * 1024 * 1024;

/// One child of a folder, as the server described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Decoded absolute path on the server (`/mokuro-reader/Series/Vol 1.cbz`),
    /// with a trailing `/` for folders.
    pub path: String,
    /// The last segment of `path`, decoded.
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub etag: Option<String>,
    pub modified: Option<String>,
}

impl Entry {
    /// What changes when the entry does: its ETag, else size and date.
    pub fn signature(&self) -> Option<String> {
        if let Some(etag) = self.etag.as_deref().map(|e| e.trim_matches('"')).filter(|e| !e.is_empty()) {
            return Some(etag.to_string());
        }
        match (self.size, self.modified.as_deref()) {
            (None, None) => None,
            (size, modified) => Some(format!("{}-{}", size.unwrap_or(0), modified.unwrap_or(""))),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    #[error("the listing is not valid XML: {0}")]
    Xml(String),
    #[error("the answer is not a WebDAV listing")]
    NotMultistatus,
}

/// `%xx` decoded, as UTF-8 (lossy: a server that encodes Latin-1 still lists).
pub fn decode_path(raw: &str) -> String {
    percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned()
}

/// The path part of an `href`, whether the server wrote a full URL or a path.
fn href_path(href: &str) -> Option<String> {
    let href = href.trim();
    let path = if let Some(rest) = href.strip_prefix("http://").or_else(|| href.strip_prefix("https://")) {
        match rest.find('/') {
            Some(slash) => &rest[slash..],
            None => "/",
        }
    } else {
        href
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.starts_with('/').then(|| decode_path(path))
}

/// `a//b/./c` → `/a/b/c`, keeping a trailing slash. `None` if it climbs out
/// with `..` or carries a control character.
pub fn normalize(path: &str) -> Option<String> {
    if path.chars().any(char::is_control) {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            _ => parts.push(part),
        }
    }
    let mut out = format!("/{}", parts.join("/"));
    if path.ends_with('/') && out != "/" {
        out.push('/');
    }
    Some(out)
}

#[derive(Default)]
struct Props {
    is_dir: bool,
    size: Option<u64>,
    etag: Option<String>,
    modified: Option<String>,
}

#[derive(Default)]
struct Response {
    href: Option<String>,
    props: Props,
}

/// Which text the parser is collecting.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Href,
    Status,
    Size,
    Etag,
    Modified,
}

/// The direct children of `folder` (a decoded absolute path) in a
/// `multistatus` body.
pub fn parse(xml: &str, folder: &str) -> Result<Vec<Entry>, ParseError> {
    let folder = normalize(folder).unwrap_or_else(|| "/".into());
    let folder = if folder.ends_with('/') { folder } else { format!("{folder}/") };

    // Not trimmed per event: an `&amp;` splits a name into several text
    // events, and trimming each would eat the spaces around it.
    let mut reader = NsReader::from_str(xml);
    let mut seen_multistatus = false;
    let mut responses: Vec<Response> = Vec::new();
    let mut current: Option<Response> = None;
    // The propstat being read: its props and whether its status said 200.
    let mut propstat: Option<(Props, bool)> = None;
    let mut field = Field::None;
    let mut text = String::new();

    loop {
        let event = reader.read_event().map_err(|e| ParseError::Xml(e.to_string()))?;
        match event {
            Event::Start(start) => {
                let local = start.local_name();
                match local.as_ref() {
                    b"multistatus" => seen_multistatus = true,
                    b"response" => current = Some(Response::default()),
                    b"propstat" if current.is_some() => propstat = Some((Props::default(), false)),
                    b"collection" => {
                        if let Some((props, _)) = propstat.as_mut() {
                            props.is_dir = true;
                        }
                    }
                    name => {
                        field = match name {
                            b"href" if current.is_some() && propstat.is_none() => Field::Href,
                            b"status" if propstat.is_some() => Field::Status,
                            b"getcontentlength" => Field::Size,
                            b"getetag" => Field::Etag,
                            b"getlastmodified" => Field::Modified,
                            _ => Field::None,
                        };
                        text.clear();
                    }
                }
            }
            Event::Empty(empty) => {
                if empty.local_name().as_ref() == b"collection" {
                    if let Some((props, _)) = propstat.as_mut() {
                        props.is_dir = true;
                    }
                }
            }
            Event::Text(chunk) if field != Field::None => {
                text.push_str(&chunk.decode().map_err(|e| ParseError::Xml(e.to_string()))?);
            }
            Event::CData(chunk) if field != Field::None => {
                text.push_str(&String::from_utf8_lossy(&chunk.into_inner()));
            }
            Event::GeneralRef(reference) if field != Field::None => {
                if let Ok(Some(c)) = reference.resolve_char_ref() {
                    text.push(c);
                } else {
                    let name = reference.decode().map_err(|e| ParseError::Xml(e.to_string()))?;
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::End(end) => {
                let local = end.local_name();
                let value = std::mem::take(&mut text);
                let value = value.trim();
                match (local.as_ref(), field) {
                    (b"href", Field::Href) => {
                        if let Some(response) = current.as_mut() {
                            response.href = Some(value.to_string());
                        }
                    }
                    (b"status", Field::Status) => {
                        if let Some((_, ok)) = propstat.as_mut() {
                            *ok = value.split_whitespace().nth(1) == Some("200");
                        }
                    }
                    (b"getcontentlength", Field::Size) => {
                        if let Some((props, _)) = propstat.as_mut() {
                            props.size = value.parse().ok();
                        }
                    }
                    (b"getetag", Field::Etag) => {
                        if let Some((props, _)) = propstat.as_mut() {
                            props.etag = Some(value.to_string()).filter(|v| !v.is_empty());
                        }
                    }
                    (b"getlastmodified", Field::Modified) => {
                        if let Some((props, _)) = propstat.as_mut() {
                            props.modified = Some(value.to_string()).filter(|v| !v.is_empty());
                        }
                    }
                    _ => {}
                }
                field = Field::None;
                match local.as_ref() {
                    b"propstat" => {
                        if let (Some((props, true)), Some(response)) = (propstat.take(), current.as_mut()) {
                            let into = &mut response.props;
                            into.is_dir |= props.is_dir;
                            into.size = into.size.or(props.size);
                            into.etag = into.etag.take().or(props.etag);
                            into.modified = into.modified.take().or(props.modified);
                        }
                        propstat = None;
                    }
                    b"response" => {
                        if let Some(response) = current.take() {
                            responses.push(response);
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen_multistatus {
        return Err(ParseError::NotMultistatus);
    }

    let mut entries = Vec::new();
    for response in responses {
        let Some(path) = response.href.as_deref().and_then(href_path) else { continue };
        let Some(path) = normalize(&path) else { continue };
        let trimmed = path.trim_end_matches('/');
        // Only direct children: never the folder itself, never deeper, never
        // something outside it.
        let Some(rest) = trimmed.strip_prefix(folder.as_str()) else { continue };
        if rest.is_empty() || rest.contains('/') {
            continue;
        }
        let is_dir = response.props.is_dir || path.ends_with('/');
        let path = if is_dir { format!("{trimmed}/") } else { trimmed.to_string() };
        entries.push(Entry {
            name: rest.to_string(),
            path,
            is_dir,
            size: if is_dir { None } else { response.props.size },
            etag: response.props.etag,
            modified: response.props.modified,
        });
    }
    entries.sort_by(|a, b| crate::manga::natural_cmp(&a.name, &b.name));
    entries.dedup_by(|a, b| a.path == b.path);
    Ok(entries)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Written by hand with the shape mokuro-bunko answers: `D:` prefix, paths
    /// percent-encoded except for some punctuation, the folder itself first,
    /// a `404` propstat for the folder's missing ETag, ETags without quotes.
    pub const BUNKO_SERIES: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:"><D:response><D:href>/mokuro-reader/Hoshi%20(Test)/</D:href><D:propstat><D:prop><D:resourcetype><D:collection /></D:resourcetype><D:getlastmodified>Mon, 12 Oct 2026 05:53:53 GMT</D:getlastmodified><D:displayname>Hoshi (Test)</D:displayname></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:getetag /></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response><D:response><D:href>/mokuro-reader/Hoshi%20(Test)/%E6%98%9F%20%E7%AC%AC01%E5%B7%BB.cbz</D:href><D:propstat><D:prop><D:resourcetype /><D:getcontentlength>37043935</D:getcontentlength><D:getcontenttype>application/vnd.comicbook+zip</D:getcontenttype><D:getlastmodified>Mon, 23 Feb 2026 20:48:53 GMT</D:getlastmodified><D:displayname>星 第01巻.cbz</D:displayname><D:getetag>1771879733.025003-37043935</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/mokuro-reader/Hoshi%20(Test)/%E6%98%9F%20%E7%AC%AC01%E5%B7%BB.mokuro</D:href><D:propstat><D:prop><D:resourcetype /><D:getcontentlength>336045</D:getcontentlength><D:getlastmodified>Mon, 12 Oct 2026 05:53:41 GMT</D:getlastmodified><D:getetag>1791784421.826940-336045</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/mokuro-reader/Hoshi%20(Test)/%E6%98%9F%20%E7%AC%AC01%E5%B7%BB.webp</D:href><D:propstat><D:prop><D:resourcetype /><D:getcontentlength>27832</D:getcontentlength><D:getcontenttype>image/webp</D:getcontenttype><D:getetag>1771531945.117989-27832</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;

    #[test]
    fn a_bunko_listing_gives_its_files_without_the_folder_itself() {
        let entries = parse(BUNKO_SERIES, "/mokuro-reader/Hoshi (Test)/").unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["星 第01巻.cbz", "星 第01巻.mokuro", "星 第01巻.webp"]);
        let cbz = &entries[0];
        assert_eq!(cbz.path, "/mokuro-reader/Hoshi (Test)/星 第01巻.cbz");
        assert!(!cbz.is_dir);
        assert_eq!(cbz.size, Some(37_043_935));
        assert_eq!(cbz.signature().as_deref(), Some("1771879733.025003-37043935"));
        // The same folder asked without its trailing slash.
        assert_eq!(parse(BUNKO_SERIES, "/mokuro-reader/Hoshi (Test)").unwrap().len(), 3);
    }

    /// Another common shape (Apache mod_dav / nginx): lowercase prefix, full
    /// URLs as hrefs, quoted ETags, a default namespace on some elements.
    const GENERIC: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:lp1="DAV:">
  <d:response>
    <d:href>https://files.example/dav/manga/</d:href>
    <d:propstat><d:prop><lp1:resourcetype><d:collection/></lp1:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response>
    <d:href>https://files.example/dav/manga/Yotsuba/</d:href>
    <d:propstat><d:prop><lp1:resourcetype><d:collection/></lp1:resourcetype><lp1:getlastmodified>Tue, 01 Sep 2026 10:00:00 GMT</lp1:getlastmodified><lp1:getetag>"abc"</lp1:getetag></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <response xmlns="DAV:">
    <href>/dav/manga/Tom%26Jerry.cbz</href>
    <propstat><prop><resourcetype/><getcontentlength>1234</getcontentlength></prop><status>HTTP/1.1 200 OK</status></propstat>
  </response>
  <d:response><d:href>/dav/manga/Yotsuba/deeper.cbz</d:href><d:propstat><d:prop><d:resourcetype/></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
  <d:response><d:href>/dav/other/escape.cbz</d:href><d:propstat><d:prop><d:resourcetype/></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
  <d:response><d:href>/dav/manga/../../etc/passwd</d:href><d:propstat><d:prop><d:resourcetype/></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
  <d:response><d:href>/dav/manga/Fish &amp; Chips/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
</d:multistatus>"#;

    #[test]
    fn other_servers_shapes_and_nothing_outside_the_folder() {
        let entries = parse(GENERIC, "/dav/manga/").unwrap();
        let summary: Vec<_> = entries.iter().map(|e| (e.name.as_str(), e.is_dir, e.size)).collect();
        assert_eq!(summary, [("Fish & Chips", true, None), ("Tom&Jerry.cbz", false, Some(1234)), ("Yotsuba", true, None)]);
        assert_eq!(entries[2].path, "/dav/manga/Yotsuba/");
        assert_eq!(entries[2].signature().as_deref(), Some("abc"));
        assert_eq!(entries[1].signature().as_deref(), Some("1234-"));
    }

    #[test]
    fn not_a_listing_is_an_error() {
        assert_eq!(parse("<html><body>Login</body></html>", "/"), Err(ParseError::NotMultistatus));
        assert!(parse("<d:multistatus xmlns:d=\"DAV:\"><d:response>", "/").is_ok_and(|e| e.is_empty()));
        assert!(matches!(parse("<a></b>", "/"), Err(ParseError::Xml(_))));
    }

    #[test]
    fn paths_are_normalized_and_never_climb() {
        assert_eq!(normalize("/a//b/./c/").as_deref(), Some("/a/b/c/"));
        assert_eq!(normalize("/a/../b"), None);
        assert_eq!(normalize("/a\u{0}b"), None);
        assert_eq!(normalize("").as_deref(), Some("/"));
        assert_eq!(href_path("https://h.example").as_deref(), Some("/"));
        assert_eq!(href_path("relative/x"), None);
        assert_eq!(decode_path("/%E6%98%9F%20x").as_str(), "/星 x");
    }
}
