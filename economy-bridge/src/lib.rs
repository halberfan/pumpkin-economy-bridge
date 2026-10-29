use std::sync::Mutex;

use economy_api::{
    BRIDGE_PLUGIN, BridgeMessage, EconomyError, EconomyRequest, EconomyResponse, EconomyResult,
    decode, encode, send,
};
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata};
use tracing::*;

/// Plugin name of the registered economy provider.
static PROVIDER: Mutex<Option<String>> = Mutex::new(None);

fn current_provider() -> Option<String> {
    PROVIDER.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn register(sender: String) -> EconomyResult<EconomyResponse> {
    let mut provider = PROVIDER.lock().unwrap_or_else(|e| e.into_inner());
    match provider.as_deref() {
        Some(existing) if existing != sender => {
            warn!("`{sender}` tried to register as economy provider, but `{existing}` already is");
            Err(EconomyError::ProviderAlreadyRegistered(existing.to_string()))
        }
        _ => {
            info!("`{sender}` registered as economy provider");
            *provider = Some(sender);
            Ok(EconomyResponse::Done)
        }
    }
}

fn unregister(sender: &str) -> EconomyResult<EconomyResponse> {
    let mut provider = PROVIDER.lock().unwrap_or_else(|e| e.into_inner());
    if provider.as_deref() == Some(sender) {
        info!("`{sender}` unregistered as economy provider");
        *provider = None;
    }
    Ok(EconomyResponse::Done)
}

fn forward(request: EconomyRequest) -> EconomyResult<EconomyResponse> {
    // The lock is released before the IPC call, which may re-enter this plugin
    let provider = current_provider().ok_or(EconomyError::NoProvider)?;
    send(&provider, &request)
}

struct EconomyBridge;

impl Plugin for EconomyBridge {
    fn new() -> Self {
        EconomyBridge
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: BRIDGE_PLUGIN.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Bjorn".into()],
            description: "Connects economy plugins with plugins that use money".into(),
            dependencies: vec![],
            permissions: vec![],
        }
    }

    fn on_load(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
        info!("Economy bridge loaded, waiting for an economy provider");
        Ok(())
    }

    fn handle_ipc_message(&self, sender: String, message: Vec<u8>) -> Result<Vec<u8>, String> {
        let result = match decode::<BridgeMessage>(&message)? {
            BridgeMessage::RegisterProvider => register(sender),
            BridgeMessage::UnregisterProvider => unregister(&sender),
            BridgeMessage::Call(request) => forward(request),
        };
        Ok(encode(&result))
    }
}

pumpkin_plugin_api::register_plugin!(EconomyBridge);
