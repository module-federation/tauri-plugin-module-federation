use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<TauriPluginModuleFederation<R>> {
    Ok(TauriPluginModuleFederation(app.clone()))
}

/// Access to the tauri-plugin-module-federation APIs.
pub struct TauriPluginModuleFederation<R: Runtime>(AppHandle<R>);

impl<R: Runtime> TauriPluginModuleFederation<R> {
    pub fn ping(&self, payload: PingRequest) -> crate::Result<PingResponse> {
        Ok(PingResponse {
            value: payload.value,
        })
    }
}
