use std::{collections::HashMap, sync::Mutex};

use sha::utils::{Digest, DigestExt};
use tauri::{
    http::uri,
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
                app.manage(Schemes::default());

                tauri::async_runtime::spawn(async move {
                    let schemes = app.state::<Schemes>();

                    let client = reqwest::Client::new();
                    let url = request.uri().clone();

                    let url: Url = url
                        .query()
                        .and_then(|query| {
                            let query_pairs: HashMap<_, _> =
                                form_urlencoded::parse(query.as_bytes())
                                    .into_iter()
                                    .collect();

                            query_pairs.get("fullUrl").map(|v| {
                                let url = Url::parse(v).unwrap();

                                let mut schemes = schemes.0.lock().unwrap();

                                let key = (url.host().unwrap().to_string(), url.port());

                                schemes.entry(key).or_insert(url.scheme().to_string());

                                url
                            })
                        })
                        .unwrap_or_else(|| {
                            let url = url.clone();

                            let host = url.host().unwrap().to_string();
                            let schemes = schemes.0.lock().unwrap();
                            let scheme = schemes
                                .get(&(host.clone(), url.port().map(|p| p.as_u16())))
                                .unwrap_or_else(|| {
                                    dbg!(&schemes);
                                    panic!("Unknown scheme for host '{host}:{:?}'", url.port())
                                });

                            let builder = uri::Builder::from(url).scheme(scheme.as_str());
                            Url::parse(&builder.build().unwrap().to_string()).unwrap()
                        });

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

            Ok(())
        })
        .build()
}
