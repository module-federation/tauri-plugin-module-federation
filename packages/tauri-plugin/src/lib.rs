use std::{collections::HashMap, sync::Mutex};

use sha::utils::{Digest, DigestExt};
use tauri::{
    http::{uri, Uri},
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};
use url::Url;

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::TauriPluginModuleFederation;
#[cfg(mobile)]
use mobile::TauriPluginModuleFederation;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the tauri-plugin-module-federation APIs.
pub trait TauriPluginModuleFederationExt<R: Runtime> {
    fn tauri_plugin_module_federation(&self) -> &TauriPluginModuleFederation<R>;
}

impl<R: Runtime, T: Manager<R>> crate::TauriPluginModuleFederationExt<R> for T {
    fn tauri_plugin_module_federation(&self) -> &TauriPluginModuleFederation<R> {
        self.state::<TauriPluginModuleFederation<R>>().inner()
    }
}

#[derive(Default)]
struct Schemes(pub Mutex<HashMap<(String, Option<u16>), String>>);

fn remote_key(url: &Url) -> (String, Option<u16>) {
    (url.host().unwrap().to_string(), url.port())
}

fn authority_from_host_port(host: &str, port: Option<u16>) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };

    match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
}

fn windows_remote_path(uri: &Uri) -> Option<(String, Option<u16>, String)> {
    let host = uri.host()?;

    let is_mf_host = host == "module-federation.localhost";
    let is_webview2 = uri.scheme_str() == Some("module-federation") && host == "localhost";
    if !is_mf_host && !is_webview2 {
        return None;
    }

    let path = uri.path().strip_prefix('/')?;
    let (authority, remote_path) = path.split_once('/').unwrap_or((path, ""));

    if authority.is_empty() {
        return None;
    }

    let remote_authority = Url::parse(&format!("http://{authority}/")).ok()?;
    let host = remote_authority.host_str()?.to_string();
    let port = remote_authority.port();
    let mut path_and_query = format!("/{remote_path}");

    if let Some(query) = uri.query() {
        path_and_query.push('?');
        path_and_query.push_str(query);
    }

    Some((host, port, path_and_query))
}

fn resolve_remote_url(uri: &Uri, schemes: &Schemes) -> Url {
    uri.query()
        .and_then(|query| {
            let query_pairs: HashMap<_, _> = form_urlencoded::parse(query.as_bytes()).collect();

            query_pairs.get("fullUrl").map(|v| {
                let url = Url::parse(v).unwrap();
                let mut schemes = schemes.0.lock().unwrap();
                schemes
                    .entry(remote_key(&url))
                    .or_insert(url.scheme().to_string());

                url
            })
        })
        .unwrap_or_else(|| {
            let (host, port, path_and_query) = windows_remote_path(uri).unwrap_or_else(|| {
                (
                    uri.host().unwrap().to_string(),
                    uri.port().map(|p| p.as_u16()),
                    uri.path_and_query()
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_else(|| "/".to_string()),
                )
            });
            let schemes = schemes.0.lock().unwrap();
            let scheme = schemes
                .get(&(host.clone(), port))
                .map(String::as_str)
                .unwrap_or_else(|| {
                    if uri.scheme_str() == Some("module-federation") {
                        "http"
                    } else {
                        "https"
                    }
                });
            let builder = uri::Builder::new()
                .scheme(scheme)
                .authority(authority_from_host_port(&host, port))
                .path_and_query(path_and_query);

            Url::parse(&builder.build().unwrap().to_string()).unwrap()
        })
}

/// Initializes the plugin.
pub fn init<R: Runtime>(arg: Option<&'static str>) -> TauriPlugin<R> {
    let builder = Builder::new("tauri-plugin-module-federation")
        .register_asynchronous_uri_scheme_protocol(
            "module-federation",
            move |app, request, responder| {
                let subfolder = arg.unwrap_or("module-federation");
                let cache_dir = app
                    .app_handle()
                    .path()
                    .app_cache_dir()
                    .expect("No cache dir!")
                    .join(subfolder);

                std::fs::create_dir_all(&cache_dir).unwrap();

                let app = app.app_handle().clone();

                tauri::async_runtime::spawn(async move {
                    let schemes = app.state::<Schemes>();
                    let client = reqwest::Client::new();
                    let url = request.uri().clone();
                    let url = resolve_remote_url(&url, schemes.inner());

                    let request_builder = client.request(request.method().clone(), url.clone());

                    let req = request_builder.build().unwrap();
                    let fetch_resp = client.execute(req).await;

                    let host = {
                        let scheme = url.scheme();
                        let host = url.host_str().unwrap();
                        let mut full_host = format!("{scheme}://{host}");

                        if let Some(port) = url.port() {
                            full_host.push_str(&format!(":{port}"));
                        }

                        full_host
                    };

                    let host_sha = sha::sha1::Sha1::default().digest(host.as_bytes()).to_hex();

                    let sha_path = cache_dir.join(host_sha);
                    let cache_path = sha_path.join(
                        sha::sha1::Sha1::default()
                            .digest(url.path().as_bytes())
                            .to_hex(),
                    );

                    let mut builder = tauri::http::Response::builder();

                    match fetch_resp {
                        Ok(mut fetch_resp) => {
                            if let Some(h) = builder.headers_mut() {
                                *h = std::mem::take(fetch_resp.headers_mut());
                            }

                            let bytes: Vec<u8> = fetch_resp.bytes().await.unwrap().into();

                            std::fs::create_dir_all(&sha_path).unwrap();

                            std::fs::write(cache_path, &bytes).ok();

                            responder.respond(builder.body(bytes).unwrap());
                        }
                        Err(_) => match std::fs::read(cache_path) {
                            Ok(bytes) => {
                                responder.respond(builder.body(bytes).unwrap());
                            }
                            Err(_) => {
                                unimplemented!()
                            }
                        },
                    }
                });
            },
        );

    #[cfg(debug_assertions)]
    let builder =
        builder.js_init_script(
            r#"""
            setTimeout(() => {
            	const federation = window.__FEDERATION__;

             	if(federation) {
              	const plugins = federation.__INSTANCES__[0]?.hooks.registerPlugins ?? {};

                if(!("tauri-module-federation-host" in plugins))
                	console.warn("[tauri-plugin-module-federation] @module-federation/tauri not found. Have you added it to your Module Federation configuration?")

              } else console.warn("[tauri-plugin-module-federation] Module Federation Runtime not found")

            }, 100);
           	"""#
            .to_string(),
        );

    builder
        .setup(|app, api| {
            #[cfg(mobile)]
            let tauri_plugin_module_federation = mobile::init(app, api)?;
            #[cfg(desktop)]
            let tauri_plugin_module_federation = desktop::init(app, api)?;
            app.manage(tauri_plugin_module_federation);
            app.manage(Schemes::default());

            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schemes_with_remote() -> Schemes {
        let schemes = Schemes::default();
        schemes
            .0
            .lock()
            .unwrap()
            .insert(("localhost".to_string(), Some(3002)), "http".to_string());
        schemes
    }

    #[test]
    fn resolves_entry_from_full_url_query() {
        let schemes = Schemes::default();
        let uri = "module-federation://localhost:3002/remoteEntry.js?fullUrl=http%3A%2F%2Flocalhost%3A3002%2FremoteEntry.js"
            .parse()
            .unwrap();

        let url = resolve_remote_url(&uri, &schemes);

        assert_eq!(url.as_str(), "http://localhost:3002/remoteEntry.js");
        assert_eq!(
            schemes
                .0
                .lock()
                .unwrap()
                .get(&("localhost".to_string(), Some(3002)))
                .map(String::as_str),
            Some("http")
        );
    }

    #[test]
    fn resolves_relative_asset_from_custom_scheme_url() {
        let schemes = schemes_with_remote();
        let uri = "module-federation://localhost:3002/static/js/chunk.js?v=1"
            .parse()
            .unwrap();

        let url = resolve_remote_url(&uri, &schemes);

        assert_eq!(url.as_str(), "http://localhost:3002/static/js/chunk.js?v=1");
    }

    #[test]
    fn resolves_relative_asset_from_windows_protocol_url() {
        let schemes = schemes_with_remote();
        let uri = "http://module-federation.localhost/localhost:3002/static/js/chunk.js?v=1"
            .parse()
            .unwrap();

        let url = resolve_remote_url(&uri, &schemes);

        assert_eq!(url.as_str(), "http://localhost:3002/static/js/chunk.js?v=1");
    }
}
