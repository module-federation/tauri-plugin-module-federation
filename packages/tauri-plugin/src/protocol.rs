//! The `mf://` protocol handler.
//!
//! Remote entries are requested as `mf://<host><path>?fullUrl=<encoded http(s) URL>`.
//! Everything an entry loads afterwards (relative chunk URLs) arrives as
//! `mf://<host><path>` and is mapped back to the scheme the entry used.
//!
//! # Network and cache
//!
//! Requests go to the network first. A successful (2xx) response is cached. When the
//! network fails, times out, or answers with an error status, the last cached copy is
//! served instead (stale-if-error), so an outage or a broken deploy never overwrites a
//! working copy. After a network failure, cached requests to that origin are answered
//! from the cache for [`UNREACHABLE_FOR`] instead of waiting for another timeout.
//!
//! # Last-known-good entries
//!
//! A remote entry that loaded successfully is committed (`mf-commit=1`, sent by the
//! runtime plugin from its `onLoad` hook) and kept as the last-known-good version.
//! When the network serves a different version, it is served as a *trial* for this
//! launch. If a remote then fails to load, the runtime plugin asks for a rollback
//! (`mf-rollback=1`): every uncommitted trial is rejected, the page reloads, and the
//! last-known-good versions are served instead. A rejected version is skipped for
//! [`REJECTION_TTL`] or until the remote publishes a different version. At most
//! [`MAX_ROLLBACKS`] rollbacks happen per launch.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

use tauri::http::{header, HeaderMap, HeaderValue, Method, Request, Response, StatusCode, Uri};
use url::Url;

use crate::cache::{content_hash, Cache, Cached, Slot, HOP_BY_HOP_HEADERS};

pub(crate) const COMMIT_PARAM: &str = "mf-commit";
pub(crate) const ROLLBACK_PARAM: &str = "mf-rollback";

/// How long a host is treated as unreachable after a network failure.
const UNREACHABLE_FOR: Duration = Duration::from_secs(30);

/// How long a version that failed to load is skipped before it gets another trial.
const REJECTION_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Rollbacks per launch. Each one makes the runtime plugin reload the page, so this
/// bounds reloads even if a remote keeps publishing new, broken versions.
const MAX_ROLLBACKS: usize = 3;

/// Header added to responses served from the cache, for debugging.
const CACHE_HEADER: &str = "x-mf-cache";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A remote entry (`fullUrl` request): versioned, may be rolled back.
    Entry,
    /// Anything an entry loads: network first, cache on failure.
    Asset,
    /// The runtime loaded this entry; keep the served version as last-known-good.
    Commit,
}

#[derive(Debug)]
pub(crate) struct Target {
    pub url: Url,
    pub mode: Mode,
}

/// Remote origin schemes by `(host, port)`, learned from entry requests.
type Schemes = Mutex<HashMap<(String, Option<u16>), String>>;

/// An entry version served during this launch.
struct Served {
    url: Url,
    hash: String,
    /// The served version is the last-known-good one.
    good: bool,
    /// A new version, different from the last-known-good one, not committed yet.
    trial: bool,
}

pub(crate) struct Protocol {
    client: reqwest::Client,
    cache: Cache,
    schemes: Schemes,
    unreachable: Mutex<HashMap<String, Instant>>,
    served: Mutex<HashMap<String, Served>>,
    rollbacks: Mutex<usize>,
    /// Whether new entry versions are tried and rolled back on failure.
    rollback_enabled: bool,
}

/// A response from the network, or a cached copy standing in for it.
struct Fetched {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
    from_cache: Option<&'static str>,
}

impl Fetched {
    fn cached(cached: Cached, kind: &'static str) -> Self {
        Self {
            status: StatusCode::OK,
            headers: cached.headers,
            body: cached.body,
            from_cache: Some(kind),
        }
    }

    /// A plain-text error standing in for an upstream response.
    fn error(status: StatusCode, message: String) -> Self {
        let response = text(status, message);
        Self {
            status,
            headers: response.headers().clone(),
            body: response.into_body(),
            from_cache: None,
        }
    }

    fn into_response(self) -> Response<Vec<u8>> {
        let mut response = response(self.status, &self.headers, self.body);
        if let Some(kind) = self.from_cache {
            response
                .headers_mut()
                .insert(CACHE_HEADER, HeaderValue::from_static(kind));
        }
        response
    }
}

impl Protocol {
    pub fn new(client: reqwest::Client, cache: Cache, rollback_enabled: bool) -> Self {
        Self {
            client,
            cache,
            schemes: Mutex::default(),
            unreachable: Mutex::default(),
            served: Mutex::default(),
            rollbacks: Mutex::default(),
            rollback_enabled,
        }
    }

    pub async fn handle(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        if has_param(request.uri(), ROLLBACK_PARAM) {
            return text(StatusCode::OK, self.rollback().to_string());
        }

        let target = match resolve(request.uri(), &self.schemes) {
            Ok(target) => target,
            Err((status, message)) => return text(status, message),
        };

        match target.mode {
            Mode::Commit => self.commit(&target.url),
            Mode::Entry => self.entry(&request, &target.url).await,
            // Chunks can import their own entry by its plain URL (Vite remotes do). Serve
            // it the same version as the entry, not whatever the network has now.
            Mode::Asset if self.lock_served().contains_key(&cache_key(&target.url)) => {
                self.entry(&request, &target.url).await
            }
            Mode::Asset => self
                .fetch_or_cache(&request, &target.url)
                .await
                .into_response(),
        }
    }

    async fn entry(&self, request: &Request<Vec<u8>>, url: &Url) -> Response<Vec<u8>> {
        let fetched = self.fetch_or_cache(request, url).await;
        if !fetched.status.is_success() {
            return fetched.into_response();
        }

        let hash = content_hash(&fetched.body);
        let good = self.cache.read(url, Slot::Good);
        let good_hash = good.as_ref().map(|good| content_hash(&good.body));
        let mut served = Served {
            url: url.clone(),
            hash: hash.clone(),
            good: good_hash.as_deref() == Some(hash.as_str()),
            trial: false,
        };

        let mut response = match good {
            Some(good) if !served.good && self.rollback_enabled => {
                if self.cache.rejected(url, REJECTION_TTL).as_deref() == Some(hash.as_str()) {
                    log::warn!("{url}: this version failed to load before, serving the last-known-good version");
                    served.hash = good_hash.unwrap_or_default();
                    served.good = true;
                    Fetched::cached(good, "last-known-good").into_response()
                } else {
                    served.trial = true;
                    fetched.into_response()
                }
            }
            _ => fetched.into_response(),
        };
        // Which version of an entry to serve is decided per launch, so the webview must
        // not keep its own copy. Otherwise a reload after a rollback reuses the rejected
        // version. Chunks are not affected and keep their upstream caching headers.
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

        self.lock_served().insert(cache_key(url), served);
        response
    }

    fn commit(&self, url: &Url) -> Response<Vec<u8>> {
        let mut served = self.lock_served();
        let Some(entry) = served.get_mut(&cache_key(url)) else {
            return text(
                StatusCode::NOT_FOUND,
                format!("{url} was not served during this launch"),
            );
        };
        if entry.good {
            return empty(StatusCode::NO_CONTENT);
        }
        match self.cache.commit(url, &entry.hash) {
            Ok(true) => {
                entry.good = true;
                entry.trial = false;
                empty(StatusCode::NO_CONTENT)
            }
            Ok(false) => text(
                StatusCode::CONFLICT,
                format!("the cached copy of {url} changed since it was served"),
            ),
            Err(error) => {
                log::warn!("failed to commit {url}: {error}");
                text(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        }
    }

    /// Rejects every uncommitted trial of this launch. Returns how many were rejected.
    fn rollback(&self) -> usize {
        let mut rollbacks = self
            .rollbacks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *rollbacks >= MAX_ROLLBACKS {
            log::warn!(
                "not rolling back: already rolled back {MAX_ROLLBACKS} times during this launch"
            );
            return 0;
        }

        let mut rejected = 0;
        for entry in self.lock_served().values_mut().filter(|entry| entry.trial) {
            match self.cache.reject(&entry.url, &entry.hash) {
                Ok(()) => {
                    log::warn!("{}: rolling back to the last-known-good version", entry.url);
                    entry.trial = false;
                    rejected += 1;
                }
                Err(error) => log::warn!("failed to reject {}: {error}", entry.url),
            }
        }
        if rejected > 0 {
            *rollbacks += 1;
        }
        rejected
    }

    /// Fetches `url`, falling back to the cached copy when the network fails or answers
    /// with an error status. Returns a gateway error when neither is available.
    async fn fetch_or_cache(&self, request: &Request<Vec<u8>>, url: &Url) -> Fetched {
        let cacheable = request.method() == Method::GET;
        let origin = url.origin().ascii_serialization();

        if cacheable && self.is_unreachable(&origin) {
            if let Some(cached) = self.cache.read(url, Slot::Current) {
                return Fetched::cached(cached, "stale");
            }
        }

        match self.fetch(request, url).await {
            Ok(fetched) => {
                self.lock_unreachable().remove(&origin);

                if fetched.status.is_success() {
                    if cacheable {
                        if let Err(error) = self.cache.write(url, &fetched.headers, &fetched.body) {
                            log::warn!("failed to cache {url}: {error}");
                        }
                    }
                    return fetched;
                }

                if cacheable {
                    if let Some(cached) = self.cache.read(url, Slot::Current) {
                        log::warn!("{url} answered {}, serving the cached copy", fetched.status);
                        return Fetched::cached(cached, "stale");
                    }
                }
                fetched
            }
            Err(error) => {
                self.lock_unreachable().insert(origin, Instant::now());

                if cacheable {
                    if let Some(cached) = self.cache.read(url, Slot::Current) {
                        log::warn!("{url} failed ({error}), serving the cached copy");
                        return Fetched::cached(cached, "stale");
                    }
                }
                let status = if error.is_timeout() {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                };
                Fetched::error(status, format!("{url} failed ({error}) and is not cached"))
            }
        }
    }

    async fn fetch(&self, request: &Request<Vec<u8>>, url: &Url) -> reqwest::Result<Fetched> {
        let mut builder = self.client.request(request.method().clone(), url.clone());

        // Forward the webview's Origin so servers with an origin allowlist
        // (e.g. the Vite dev server) answer with CORS headers. ES module
        // remotes are loaded in CORS mode and fail without them.
        if let Some(origin) = request.headers().get(header::ORIGIN) {
            builder = builder.header(header::ORIGIN, origin.clone());
        }
        if !request.body().is_empty() {
            builder = builder.body(request.body().clone());
        }

        let mut upstream = builder.send().await?;
        let status = upstream.status();
        let headers = std::mem::take(upstream.headers_mut());
        // A body that fails halfway counts as a network failure, not a success.
        let body = upstream.bytes().await?.to_vec();

        Ok(Fetched {
            status,
            headers,
            body,
            from_cache: None,
        })
    }

    fn is_unreachable(&self, origin: &str) -> bool {
        self.lock_unreachable()
            .get(origin)
            .is_some_and(|since| since.elapsed() < UNREACHABLE_FOR)
    }

    fn lock_unreachable(&self) -> MutexGuard<'_, HashMap<String, Instant>> {
        self.unreachable
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_served(&self) -> MutexGuard<'_, HashMap<String, Served>> {
        self.served
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn query_params(uri: &Uri) -> HashMap<String, String> {
    uri.query()
        .map(|query| {
            form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

fn has_param(uri: &Uri, name: &str) -> bool {
    query_params(uri).contains_key(name)
}

/// The cache ignores query strings, so served versions are tracked the same way.
fn cache_key(url: &Url) -> String {
    format!("{}{}", url.origin().ascii_serialization(), url.path())
}

/// Maps an `mf://` request URI to the remote URL it stands for.
pub(crate) fn resolve(uri: &Uri, schemes: &Schemes) -> Result<Target, (StatusCode, String)> {
    let params = query_params(uri);
    let mut schemes = schemes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(full_url) = params.get("fullUrl") {
        let url = Url::parse(full_url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("fullUrl must be an http(s) URL, got {full_url:?}"),
                )
            })?;

        let host = url.host_str().unwrap_or_default().to_string();
        schemes.insert((host, url.port()), url.scheme().to_string());

        let mode = if params.contains_key(COMMIT_PARAM) {
            Mode::Commit
        } else {
            Mode::Entry
        };
        return Ok(Target { url, mode });
    }

    let (Some(host), Some(authority)) = (uri.host(), uri.authority()) else {
        return Err((StatusCode::BAD_REQUEST, format!("{uri} has no host")));
    };
    let scheme = schemes
        .get(&(host.to_string(), uri.port_u16()))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("{uri}: no remote entry has been loaded from {authority} yet"),
            )
        })?;
    let path_and_query = uri.path_and_query().map_or("/", |p| p.as_str());

    Url::parse(&format!("{scheme}://{authority}{path_and_query}"))
        .map(|url| Target {
            url,
            mode: Mode::Asset,
        })
        .map_err(|error| (StatusCode::BAD_REQUEST, format!("{uri}: {error}")))
}

fn response(status: StatusCode, headers: &HeaderMap, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let target = response.headers_mut();
    for (name, value) in headers {
        if !HOP_BY_HOP_HEADERS.contains(&name.as_str()) {
            target.append(name.clone(), value.clone());
        }
    }
    response
}

/// A plain-text response. Allows any origin so the runtime plugin can read it.
pub(crate) fn text(status: StatusCode, message: impl Into<String>) -> Response<Vec<u8>> {
    let mut response = empty(status);
    *response.body_mut() = message.into().into_bytes();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(value: &str) -> Uri {
        value.parse().unwrap()
    }

    fn entry_uri(full_url: &str, extra: &str) -> Uri {
        let encoded: String = form_urlencoded::byte_serialize(full_url.as_bytes()).collect();
        uri(&format!(
            "mf://cdn.example.com/app/remoteEntry.js?fullUrl={encoded}{extra}"
        ))
    }

    #[test]
    fn entry_requests_use_the_full_url_including_its_query() {
        let schemes = Schemes::default();

        let target = resolve(
            &entry_uri("https://cdn.example.com/app/remoteEntry.js?v=1&t=2", ""),
            &schemes,
        )
        .unwrap();

        assert_eq!(
            target.url.as_str(),
            "https://cdn.example.com/app/remoteEntry.js?v=1&t=2"
        );
        assert_eq!(target.mode, Mode::Entry);
    }

    #[test]
    fn chunk_requests_reuse_the_scheme_of_the_entry() {
        let schemes = Schemes::default();
        resolve(
            &entry_uri("http://localhost:3002/remoteEntry.js", ""),
            &schemes,
        )
        .unwrap();

        let target = resolve(&uri("mf://localhost:3002/static/js/a.js?x=1"), &schemes).unwrap();

        assert_eq!(
            target.url.as_str(),
            "http://localhost:3002/static/js/a.js?x=1"
        );
        assert_eq!(target.mode, Mode::Asset);
    }

    #[test]
    fn chunk_requests_on_default_ports_resolve() {
        let schemes = Schemes::default();
        resolve(
            &entry_uri("https://cdn.example.com/app/remoteEntry.js", ""),
            &schemes,
        )
        .unwrap();

        let target = resolve(&uri("mf://cdn.example.com/app/chunk.js"), &schemes).unwrap();

        assert_eq!(target.url.as_str(), "https://cdn.example.com/app/chunk.js");
    }

    #[test]
    fn chunk_requests_for_unknown_hosts_fail_cleanly() {
        let (status, _) =
            resolve(&uri("mf://localhost:3002/a.js"), &Schemes::default()).unwrap_err();

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn commit_flag_selects_commit_mode() {
        let schemes = Schemes::default();
        let entry = "https://cdn.example.com/app/remoteEntry.js";

        let commit = resolve(&entry_uri(entry, "&mf-commit=1"), &schemes).unwrap();

        assert_eq!(commit.mode, Mode::Commit);
        assert_eq!(commit.url.as_str(), entry);
    }

    #[test]
    fn rollback_requests_are_recognized() {
        assert!(has_param(
            &uri("mf://localhost/?mf-rollback=1"),
            ROLLBACK_PARAM
        ));
        assert!(!has_param(&uri("mf://localhost/a.js"), ROLLBACK_PARAM));
    }

    #[test]
    fn non_http_full_urls_are_rejected() {
        for full_url in ["file:///etc/passwd", "not a url", "mf://localhost/x"] {
            let (status, _) = resolve(&entry_uri(full_url, ""), &Schemes::default()).unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST, "{full_url}");
        }
    }

    #[test]
    fn cache_keys_ignore_the_query_string() {
        let a = Url::parse("https://cdn.example.com/remoteEntry.js?t=1").unwrap();
        let b = Url::parse("https://cdn.example.com/remoteEntry.js?t=2").unwrap();

        assert_eq!(cache_key(&a), cache_key(&b));
    }

    #[test]
    fn cached_responses_drop_connection_headers_and_are_marked() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/javascript"));
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));

        let response = Fetched::cached(
            Cached {
                headers,
                body: b"x".to_vec(),
            },
            "stale",
        )
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/javascript");
        assert_eq!(response.headers()[CACHE_HEADER], "stale");
        assert!(!response.headers().contains_key("transfer-encoding"));
    }

    /// A one-thread HTTP server whose response can be swapped between requests.
    struct TestServer {
        port: u16,
        response: std::sync::Arc<Mutex<(u16, Vec<u8>)>>,
    }

    impl TestServer {
        fn start() -> Self {
            use std::io::{BufRead, BufReader, Write};

            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let response = std::sync::Arc::new(Mutex::new((200, Vec::new())));
            let shared = response.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                        line.clear();
                    }
                    let (status, body) = shared.lock().unwrap().clone();
                    let mut stream = &stream;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} X\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                }
            });
            Self { port, response }
        }

        fn serve(&self, status: u16, body: &[u8]) {
            *self.response.lock().unwrap() = (status, body.to_vec());
        }
    }

    fn launch(root: &std::path::Path) -> Protocol {
        Protocol::new(reqwest::Client::new(), Cache::new(root.to_path_buf()), true)
    }

    fn request(protocol: &Protocol, uri: &Uri) -> Response<Vec<u8>> {
        let request = Request::builder()
            .uri(uri.clone())
            .body(Vec::new())
            .unwrap();
        tauri::async_runtime::block_on(protocol.handle(request))
    }

    #[test]
    fn failed_releases_roll_back_to_the_last_known_good_version() {
        let root =
            std::env::temp_dir().join(format!("tauri-plugin-mf-protocol-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let server = TestServer::start();
        let full_url = format!("http://127.0.0.1:{}/remoteEntry.js", server.port);
        let entry = entry_uri(&full_url, "");
        let commit = entry_uri(&full_url, "&mf-commit=1");
        let rollback = uri("mf://localhost/?mf-rollback=1");

        // Launch 1: v1 loads and is committed.
        server.serve(200, b"v1");
        let first = launch(&root);
        let served = request(&first, &entry);
        assert_eq!(served.body(), b"v1");
        assert_eq!(served.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(request(&first, &commit).status(), StatusCode::NO_CONTENT);

        // Launch 2: v2 is published and fails to load. Rolling back serves v1 again.
        server.serve(200, b"v2");
        let second = launch(&root);
        assert_eq!(request(&second, &entry).body(), b"v2");
        assert_eq!(request(&second, &rollback).body(), b"1");
        let reloaded = request(&second, &entry);
        assert_eq!(reloaded.body(), b"v1");
        assert_eq!(reloaded.headers()[CACHE_HEADER], "last-known-good");
        assert_eq!(
            request(&second, &rollback).body(),
            b"0",
            "nothing left to roll back"
        );
        // A chunk importing the entry by its plain URL gets the same version.
        let plain = uri(&format!("mf://127.0.0.1:{}/remoteEntry.js", server.port));
        assert_eq!(request(&second, &plain).body(), b"v1");

        // Launch 3: v2 is still published; it stays rejected after a restart.
        let third = launch(&root);
        assert_eq!(request(&third, &entry).body(), b"v1");
        // Committing while serving the last-known-good version keeps it.
        assert_eq!(request(&third, &commit).status(), StatusCode::NO_CONTENT);

        // Launch 4: v3 fixes the release; it gets a trial and becomes the new good version.
        server.serve(200, b"v3");
        let fourth = launch(&root);
        assert_eq!(request(&fourth, &entry).body(), b"v3");
        assert_eq!(request(&fourth, &commit).status(), StatusCode::NO_CONTENT);
        assert_eq!(
            request(&fourth, &rollback).body(),
            b"0",
            "committed versions are kept"
        );

        // Launch 5: the server breaks; the last successful copy is served.
        server.serve(500, b"Internal Server Error");
        let fifth = launch(&root);
        let stale = request(&fifth, &entry);
        assert_eq!(stale.status(), StatusCode::OK);
        assert_eq!(stale.body(), b"v3");
        assert_eq!(stale.headers()[CACHE_HEADER], "stale");

        // An asset that was never cached passes the upstream error through.
        let asset = uri(&format!("mf://127.0.0.1:{}/missing.js", server.port));
        assert_eq!(
            request(&fifth, &asset).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn disabled_rollback_always_serves_the_network_version() {
        let root =
            std::env::temp_dir().join(format!("tauri-plugin-mf-norollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let server = TestServer::start();
        let full_url = format!("http://127.0.0.1:{}/remoteEntry.js", server.port);
        let entry = entry_uri(&full_url, "");
        let dev = |root: &std::path::Path| {
            Protocol::new(
                reqwest::Client::new(),
                Cache::new(root.to_path_buf()),
                false,
            )
        };

        server.serve(200, b"v1");
        let first = dev(&root);
        request(&first, &entry);
        request(&first, &entry_uri(&full_url, "&mf-commit=1"));

        server.serve(200, b"v2");
        let second = dev(&root);
        assert_eq!(request(&second, &entry).body(), b"v2");
        assert_eq!(
            request(&second, &uri("mf://localhost/?mf-rollback=1")).body(),
            b"0"
        );
        assert_eq!(request(&second, &entry).body(), b"v2");
    }

    #[test]
    fn rollbacks_per_launch_are_capped() {
        let root = std::env::temp_dir().join(format!("tauri-plugin-mf-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let server = TestServer::start();
        let full_url = format!("http://127.0.0.1:{}/remoteEntry.js", server.port);
        let entry = entry_uri(&full_url, "");
        let rollback = uri("mf://localhost/?mf-rollback=1");

        server.serve(200, b"good");
        let first = launch(&root);
        request(&first, &entry);
        request(&first, &entry_uri(&full_url, "&mf-commit=1"));

        // A remote that publishes a new broken version on every request.
        let second = launch(&root);
        for attempt in 0..MAX_ROLLBACKS {
            server.serve(200, format!("broken {attempt}").as_bytes());
            request(&second, &entry);
            assert_eq!(
                request(&second, &rollback).body(),
                b"1",
                "attempt {attempt}"
            );
        }
        server.serve(200, b"broken again");
        request(&second, &entry);
        assert_eq!(request(&second, &rollback).body(), b"0", "capped");
    }

    #[test]
    fn unreachable_remotes_without_a_cached_copy_fail_with_a_gateway_error() {
        let root =
            std::env::temp_dir().join(format!("tauri-plugin-mf-offline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Bind and drop a listener to get a port with nothing listening on it.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();

        let response = request(
            &launch(&root),
            &entry_uri(&format!("http://127.0.0.1:{port}/remoteEntry.js"), ""),
        );

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }
}
