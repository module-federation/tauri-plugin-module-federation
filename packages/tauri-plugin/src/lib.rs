use std::time::Duration;

use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod cache;
mod error;
mod models;
mod protocol;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::TauriPluginModuleFederation;
#[cfg(mobile)]
use mobile::TauriPluginModuleFederation;

use protocol::Protocol;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the tauri-plugin-module-federation APIs.
pub trait TauriPluginModuleFederationExt<R: Runtime> {
    fn tauri_plugin_module_federation(&self) -> &TauriPluginModuleFederation<R>;
}

impl<R: Runtime, T: Manager<R>> crate::TauriPluginModuleFederationExt<R> for T {
    fn tauri_plugin_module_federation(&self) -> &TauriPluginModuleFederation<R> {
        self.state::<TauriPluginModuleFederation<R>>().inner()
    }
}

/// The custom protocol that `@module-federation/tauri` rewrites remote entries to.
const SCHEME: &str = "mf";

/// Configures the plugin.
///
/// ```no_run
/// use std::time::Duration;
///
/// let app = tauri::Builder::<tauri::Wry>::default().plugin(
///     tauri_plugin_module_federation::Builder::new()
///         .cache_folder("remotes")
///         .connect_timeout(Duration::from_secs(5))
///         .build(),
/// );
/// ```
#[derive(Debug, Clone)]
pub struct Builder {
    cache_folder: String,
    connect_timeout: Duration,
    read_timeout: Duration,
    rollback: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            cache_folder: "module-federation".into(),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(10),
            rollback: !cfg!(debug_assertions),
        }
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Folder inside the app cache dir that holds cached remotes. Defaults to `module-federation`.
    pub fn cache_folder(mut self, folder: impl Into<String>) -> Self {
        self.cache_folder = folder.into();
        self
    }

    /// How long to wait for a connection to a remote before serving it from the cache.
    /// Defaults to 10 seconds.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// How long a remote may stay silent while sending a response before it is served from
    /// the cache. This is an inactivity timeout, so large downloads on slow networks are
    /// not cut off. Defaults to 10 seconds.
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Whether a new version of a remote entry that fails to load is rolled back to the
    /// last version that loaded successfully, in this launch and the following ones.
    ///
    /// Defaults to `true` in release builds and `false` in debug builds, so that during
    /// development a broken remote shows its error instead of an older version.
    /// Serving cached copies when the network fails is always enabled.
    pub fn rollback(mut self, enabled: bool) -> Self {
        self.rollback = enabled;
        self
    }

    pub fn build<R: Runtime>(self) -> TauriPlugin<R> {
        let builder = PluginBuilder::new("tauri-plugin-module-federation")
            .register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
                let app = ctx.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let response = match app.try_state::<Protocol>() {
                        Some(protocol) => protocol.handle(request).await,
                        None => protocol::text(
                            tauri::http::StatusCode::INTERNAL_SERVER_ERROR,
                            "tauri-plugin-module-federation is not initialized",
                        ),
                    };
                    responder.respond(response);
                });
            });

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
            .setup(move |app, api| {
                let cache_dir = app.path().app_cache_dir()?.join(&self.cache_folder);
                if let Err(error) = std::fs::create_dir_all(&cache_dir) {
                    log::warn!("failed to create {}: {error}", cache_dir.display());
                }
                let client = reqwest::Client::builder()
                    .connect_timeout(self.connect_timeout)
                    .read_timeout(self.read_timeout)
                    .build()?;
                app.manage(Protocol::new(
                    client,
                    cache::Cache::new(cache_dir),
                    self.rollback,
                ));

                #[cfg(mobile)]
                let tauri_plugin_module_federation = mobile::init(app, api)?;
                #[cfg(desktop)]
                let tauri_plugin_module_federation = desktop::init(app, api)?;
                app.manage(tauri_plugin_module_federation);

                Ok(())
            })
            .build()
    }
}

/// Initializes the plugin with default settings.
///
/// `cache_folder` is the folder inside the app cache dir that holds cached remotes
/// (`module-federation` when `None`). Use [`Builder`] to configure timeouts.
pub fn init<R: Runtime>(cache_folder: Option<&'static str>) -> TauriPlugin<R> {
    match cache_folder {
        Some(folder) => Builder::new().cache_folder(folder),
        None => Builder::new(),
    }
    .build()
}
