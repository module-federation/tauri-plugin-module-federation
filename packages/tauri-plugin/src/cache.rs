//! On-disk cache for remote assets.
//!
//! Layout, relative to the cache root:
//!
//! ```text
//! <sha1(scheme://host[:port])>/<sha1(path)>               body of the last successful (2xx) response
//! <sha1(scheme://host[:port])>/<sha1(path)>.headers       its headers, one `name: value` per line
//! <sha1(scheme://host[:port])>/<sha1(path)>.good          last-known-good copy, written on commit
//! <sha1(scheme://host[:port])>/<sha1(path)>.good.headers
//! <sha1(scheme://host[:port])>/<sha1(path)>.rejected      sha1 of a version that failed to load
//! ```
//!
//! The query string is not part of the key, so cache-busting parameters such as
//! `?t=<timestamp>` still hit the cache when offline. Entries written before headers
//! were cached have no `.headers` file and are served without headers.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use sha::utils::{Digest, DigestExt};
use tauri::http::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

/// Headers that describe the original connection rather than the body.
pub(crate) const HOP_BY_HOP_HEADERS: &[&str] = &["connection", "keep-alive", "transfer-encoding"];

/// Which copy of a cached URL to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The last successful response from the network.
    Current,
    /// The copy committed after the runtime confirmed that it loaded.
    Good,
}

impl Slot {
    fn suffix(self) -> &'static str {
        match self {
            Slot::Current => "",
            Slot::Good => ".good",
        }
    }
}

/// A cached response.
#[derive(Debug)]
pub(crate) struct Cached {
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

pub(crate) struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn body_path(&self, url: &Url, slot: Slot) -> PathBuf {
        let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
        if let Some(port) = url.port() {
            origin.push_str(&format!(":{port}"));
        }
        let name = format!("{}{}", sha1_hex(url.path()), slot.suffix());
        self.root.join(sha1_hex(&origin)).join(name)
    }

    pub fn read(&self, url: &Url, slot: Slot) -> Option<Cached> {
        let body_path = self.body_path(url, slot);
        let body = fs::read(&body_path).ok()?;
        let headers = fs::read_to_string(headers_path(&body_path))
            .map(|stored| deserialize_headers(&stored))
            .unwrap_or_default();
        Some(Cached { headers, body })
    }

    /// Stores a successful response as the current copy of `url`.
    pub fn write(&self, url: &Url, headers: &HeaderMap, body: &[u8]) -> io::Result<()> {
        let body_path = self.body_path(url, Slot::Current);
        if let Some(dir) = body_path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_atomic(
            &headers_path(&body_path),
            serialize_headers(headers).as_bytes(),
        )?;
        write_atomic(&body_path, body)
    }

    /// Copies the current copy of `url` to its last-known-good slot, if its content still
    /// has `expected_hash`. Returns `false` when there is no such current copy.
    pub fn commit(&self, url: &Url, expected_hash: &str) -> io::Result<bool> {
        let Some(current) = self.read(url, Slot::Current) else {
            return Ok(false);
        };
        if content_hash(&current.body) != expected_hash {
            return Ok(false);
        }
        let good_path = self.body_path(url, Slot::Good);
        write_atomic(
            &headers_path(&good_path),
            serialize_headers(&current.headers).as_bytes(),
        )?;
        write_atomic(&good_path, &current.body)?;
        Ok(true)
    }

    /// Records that the version of `url` with `hash` failed to load.
    pub fn reject(&self, url: &Url, hash: &str) -> io::Result<()> {
        let path = self.rejected_path(url);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_atomic(&path, hash.as_bytes())
    }

    /// The hash of the version of `url` that failed to load, if it failed within `ttl`.
    pub fn rejected(&self, url: &Url, ttl: Duration) -> Option<String> {
        let path = self.rejected_path(url);
        let age = fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .unwrap_or(Duration::ZERO);
        if age >= ttl {
            return None;
        }
        fs::read_to_string(path).ok()
    }

    fn rejected_path(&self, url: &Url) -> PathBuf {
        let mut path = self.body_path(url, Slot::Current).into_os_string();
        path.push(".rejected");
        PathBuf::from(path)
    }
}

/// Identifies a version of a cached body.
pub(crate) fn content_hash(body: &[u8]) -> String {
    sha::sha1::Sha1::default().digest(body).to_hex()
}

fn sha1_hex(value: &str) -> String {
    sha::sha1::Sha1::default().digest(value.as_bytes()).to_hex()
}

fn headers_path(body_path: &Path) -> PathBuf {
    let mut name = body_path.as_os_str().to_owned();
    name.push(".headers");
    PathBuf::from(name)
}

/// Writes through a temporary file and a rename, so readers never see a partial file.
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, contents)?;
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

/// Serializes headers as `name: value` lines. Non-UTF-8 values are skipped.
pub(crate) fn serialize_headers(headers: &HeaderMap) -> String {
    headers
        .iter()
        .filter(|(name, _)| !HOP_BY_HOP_HEADERS.contains(&name.as_str()))
        .filter_map(|(name, value)| Some(format!("{name}: {}\n", value.to_str().ok()?)))
        .collect()
}

/// Parses headers written by [`serialize_headers`], skipping invalid lines.
pub(crate) fn deserialize_headers(stored: &str) -> HeaderMap {
    stored
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((
                HeaderName::from_bytes(name.trim().as_bytes()).ok()?,
                HeaderValue::from_str(value.trim()).ok()?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(name: &str) -> Cache {
        let root = std::env::temp_dir().join(format!(
            "tauri-plugin-mf-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        Cache::new(root)
    }

    fn headers(content_type: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static(content_type));
        headers
    }

    #[test]
    fn headers_round_trip_without_connection_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/javascript"));
        headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        headers.append("set-cookie", HeaderValue::from_static("a=1"));
        headers.append("set-cookie", HeaderValue::from_static("b=2"));

        let restored = deserialize_headers(&serialize_headers(&headers));

        assert_eq!(restored["content-type"], "text/javascript");
        assert_eq!(restored["access-control-allow-origin"], "*");
        assert_eq!(restored.get_all("set-cookie").iter().count(), 2);
        assert!(!restored.contains_key("transfer-encoding"));
    }

    #[test]
    fn invalid_header_lines_are_skipped() {
        let restored = deserialize_headers("content-type: text/css\nnot a header\n: empty\n");

        assert_eq!(restored.len(), 1);
        assert_eq!(restored["content-type"], "text/css");
    }

    #[test]
    fn write_then_read_current() {
        let cache = temp_cache("write-read");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();

        cache
            .write(&url, &headers("text/javascript"), b"v1")
            .unwrap();
        let cached = cache.read(&url, Slot::Current).unwrap();

        assert_eq!(cached.body, b"v1");
        assert_eq!(cached.headers["content-type"], "text/javascript");
        assert!(cache.read(&url, Slot::Good).is_none());
    }

    #[test]
    fn query_string_is_not_part_of_the_key() {
        let cache = temp_cache("query");
        let stored = Url::parse("https://cdn.example.com/app/remoteEntry.js?t=1").unwrap();
        let requested = Url::parse("https://cdn.example.com/app/remoteEntry.js?t=2").unwrap();

        cache.write(&stored, &HeaderMap::new(), b"entry").unwrap();

        assert_eq!(
            cache.read(&requested, Slot::Current).unwrap().body,
            b"entry"
        );
    }

    #[test]
    fn origins_do_not_share_entries() {
        let cache = temp_cache("origins");
        let http = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();
        let other_port = Url::parse("http://localhost:3003/remoteEntry.js").unwrap();
        let https = Url::parse("https://localhost:3002/remoteEntry.js").unwrap();

        cache.write(&http, &HeaderMap::new(), b"3002").unwrap();

        assert!(cache.read(&other_port, Slot::Current).is_none());
        assert!(cache.read(&https, Slot::Current).is_none());
    }

    #[test]
    fn commit_keeps_the_good_copy_when_current_changes() {
        let cache = temp_cache("commit");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();

        assert!(
            !cache.commit(&url, &content_hash(b"v1")).unwrap(),
            "nothing to commit yet"
        );

        cache
            .write(&url, &headers("text/javascript"), b"v1")
            .unwrap();
        assert!(cache.commit(&url, &content_hash(b"v1")).unwrap());
        cache.write(&url, &headers("text/plain"), b"v2").unwrap();

        assert_eq!(cache.read(&url, Slot::Current).unwrap().body, b"v2");
        let good = cache.read(&url, Slot::Good).unwrap();
        assert_eq!(good.body, b"v1");
        assert_eq!(good.headers["content-type"], "text/javascript");
    }

    #[test]
    fn entries_without_headers_file_are_still_served() {
        let cache = temp_cache("legacy");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();
        let body_path = cache.body_path(&url, Slot::Current);
        fs::create_dir_all(body_path.parent().unwrap()).unwrap();
        fs::write(&body_path, b"legacy").unwrap();

        let cached = cache.read(&url, Slot::Current).unwrap();

        assert_eq!(cached.body, b"legacy");
        assert!(cached.headers.is_empty());
    }

    #[test]
    fn writes_leave_no_temporary_files() {
        let cache = temp_cache("atomic");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();

        cache.write(&url, &HeaderMap::new(), b"v1").unwrap();
        cache.commit(&url, &content_hash(b"v1")).unwrap();
        cache.reject(&url, &content_hash(b"v2")).unwrap();

        let dir = cache.body_path(&url, Slot::Current);
        let leftovers: Vec<_> = fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn commit_refuses_a_different_version() {
        let cache = temp_cache("commit-mismatch");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();

        cache.write(&url, &HeaderMap::new(), b"v2").unwrap();

        assert!(!cache.commit(&url, &content_hash(b"v1")).unwrap());
        assert!(cache.read(&url, Slot::Good).is_none());
    }

    #[test]
    fn rejections_are_remembered_until_they_expire() {
        let cache = temp_cache("reject");
        let url = Url::parse("http://localhost:3002/remoteEntry.js").unwrap();
        let hash = content_hash(b"v2");

        assert!(cache.rejected(&url, Duration::from_secs(60)).is_none());
        cache.reject(&url, &hash).unwrap();

        assert_eq!(cache.rejected(&url, Duration::from_secs(60)), Some(hash));
        assert!(cache.rejected(&url, Duration::ZERO).is_none(), "expired");
    }
}
