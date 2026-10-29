//! Shared protocol between the `economy_bridge` plugin, economy providers and
//! plugins that need money.
//!
//! Nobody talks to an economy plugin directly. Every call goes through the
//! bridge over IPC, so consumers work with whichever provider is installed:
//!
//! ```text
//! consumer --BridgeMessage::Call--> economy_bridge --EconomyRequest--> provider
//! ```
//!
//! - Consumers use the functions in [`client`].
//! - Providers implement [`provider::EconomyProvider`], call
//!   [`provider::register`] in `on_load` and forward `handle_ipc_message` to
//!   [`provider::handle_message`].
//!
//! Amounts are integers in the currency's smallest unit (e.g. cents), so no
//! money gets lost to floating point rounding.

use std::fmt;

use pumpkin_plugin_api::{Player, ipc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Plugin name of the bridge, used as the IPC recipient.
pub const BRIDGE_PLUGIN: &str = "economy_bridge";

/// Money in the currency's smallest unit.
pub type Amount = i64;

pub type EconomyResult<T> = Result<T, EconomyError>;

/// Operations every economy provider has to support.
/// Accounts are identified by the player's UUID string (see [`account_of`]).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EconomyRequest {
    Balance { account: String },
    Has { account: String, amount: Amount },
    Deposit { account: String, amount: Amount },
    Withdraw { account: String, amount: Amount },
    Transfer { from: String, to: String, amount: Amount },
    Format { amount: Amount },
    ProviderName,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EconomyResponse {
    Amount(Amount),
    Bool(bool),
    Text(String),
    Done,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EconomyError {
    /// No economy plugin has registered with the bridge.
    NoProvider,
    /// Another economy plugin is already registered.
    ProviderAlreadyRegistered(String),
    InsufficientFunds { balance: Amount },
    /// Amount was zero or negative.
    InvalidAmount,
    /// IPC failed or a message could not be decoded.
    Transport(String),
    /// Provider-specific failure.
    Provider(String),
}

impl fmt::Display for EconomyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProvider => write!(f, "no economy plugin is installed"),
            Self::ProviderAlreadyRegistered(name) => {
                write!(f, "economy plugin `{name}` is already registered")
            }
            Self::InsufficientFunds { .. } => write!(f, "not enough money"),
            Self::InvalidAmount => write!(f, "amount must be positive"),
            Self::Transport(e) => write!(f, "economy bridge error: {e}"),
            Self::Provider(e) => write!(f, "economy error: {e}"),
        }
    }
}

impl std::error::Error for EconomyError {}

/// Messages sent to the bridge.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BridgeMessage {
    /// Registers the sending plugin as the economy provider.
    RegisterProvider,
    /// Removes the sending plugin as provider.
    UnregisterProvider,
    /// Forwards a request to the registered provider.
    Call(EconomyRequest),
}

/// Account id of a player.
pub fn account_of(player: &Player) -> String {
    pumpkin_plugin_api::uuid::to_string(player.get_id())
}

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    // These types only contain strings, numbers and enums, so this cannot fail
    serde_json::to_vec(value).unwrap_or_default()
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid economy message: {e}"))
}

/// Sends `message` to `recipient` over IPC and decodes its economy reply.
pub fn send(recipient: &str, message: &impl Serialize) -> EconomyResult<EconomyResponse> {
    let reply = match ipc::send_ipc_message(recipient, &encode(message)) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return Err(EconomyError::Transport(e)),
        Err(()) => {
            return Err(EconomyError::Transport(format!(
                "plugin `{recipient}` is not loaded"
            )));
        }
    };
    decode::<EconomyResult<EconomyResponse>>(&reply).map_err(EconomyError::Transport)?
}

fn unexpected(response: EconomyResponse) -> EconomyError {
    EconomyError::Transport(format!("unexpected response {response:?}"))
}

/// Functions for plugins that use the economy.
pub mod client {
    use super::*;

    fn call(request: EconomyRequest) -> EconomyResult<EconomyResponse> {
        send(BRIDGE_PLUGIN, &BridgeMessage::Call(request))
    }

    fn amount(request: EconomyRequest) -> EconomyResult<Amount> {
        match call(request)? {
            EconomyResponse::Amount(amount) => Ok(amount),
            other => Err(unexpected(other)),
        }
    }

    /// Whether an economy plugin is registered and reachable.
    pub fn is_available() -> bool {
        provider_name().is_ok()
    }

    pub fn provider_name() -> EconomyResult<String> {
        match call(EconomyRequest::ProviderName)? {
            EconomyResponse::Text(name) => Ok(name),
            other => Err(unexpected(other)),
        }
    }

    pub fn balance(account: &str) -> EconomyResult<Amount> {
        amount(EconomyRequest::Balance { account: account.to_string() })
    }

    pub fn has(account: &str, amount: Amount) -> EconomyResult<bool> {
        match call(EconomyRequest::Has { account: account.to_string(), amount })? {
            EconomyResponse::Bool(has) => Ok(has),
            other => Err(unexpected(other)),
        }
    }

    /// Adds money and returns the new balance.
    pub fn deposit(account: &str, amount: Amount) -> EconomyResult<Amount> {
        super::client::amount(EconomyRequest::Deposit { account: account.to_string(), amount })
    }

    /// Removes money and returns the new balance.
    /// Fails with [`EconomyError::InsufficientFunds`] if the account has too little.
    pub fn withdraw(account: &str, amount: Amount) -> EconomyResult<Amount> {
        super::client::amount(EconomyRequest::Withdraw { account: account.to_string(), amount })
    }

    pub fn transfer(from: &str, to: &str, amount: Amount) -> EconomyResult<()> {
        match call(EconomyRequest::Transfer {
            from: from.to_string(),
            to: to.to_string(),
            amount,
        })? {
            EconomyResponse::Done => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// Formats an amount in the provider's currency, e.g. `12.50 Coins`.
    pub fn format(amount: Amount) -> EconomyResult<String> {
        match call(EconomyRequest::Format { amount })? {
            EconomyResponse::Text(text) => Ok(text),
            other => Err(unexpected(other)),
        }
    }
}

/// Everything an economy plugin needs to plug into the bridge.
pub mod provider {
    use super::*;

    /// Implemented by economy plugins. Amounts passed to `deposit`, `withdraw`
    /// and `transfer` are already checked to be positive.
    pub trait EconomyProvider: Send + Sync {
        fn name(&self) -> String;
        fn balance(&self, account: &str) -> EconomyResult<Amount>;
        /// Returns the new balance.
        fn deposit(&self, account: &str, amount: Amount) -> EconomyResult<Amount>;
        /// Returns the new balance.
        fn withdraw(&self, account: &str, amount: Amount) -> EconomyResult<Amount>;
        fn format(&self, amount: Amount) -> String;

        fn has(&self, account: &str, amount: Amount) -> EconomyResult<bool> {
            Ok(self.balance(account)? >= amount)
        }

        /// Default implementation withdraws, then deposits and refunds if the
        /// deposit fails. Override it if your storage can do this atomically.
        fn transfer(&self, from: &str, to: &str, amount: Amount) -> EconomyResult<()> {
            self.withdraw(from, amount)?;
            if let Err(e) = self.deposit(to, amount) {
                self.deposit(from, amount)?;
                return Err(e);
            }
            Ok(())
        }
    }

    /// Registers this plugin as the economy provider. Call it in `on_load`.
    pub fn register() -> EconomyResult<()> {
        send(BRIDGE_PLUGIN, &BridgeMessage::RegisterProvider).map(|_| ())
    }

    /// Call it in `on_unload`.
    pub fn unregister() -> EconomyResult<()> {
        send(BRIDGE_PLUGIN, &BridgeMessage::UnregisterProvider).map(|_| ())
    }

    /// Answers a request from the bridge. Call it from `handle_ipc_message`.
    pub fn handle_message(
        provider: &dyn EconomyProvider,
        sender: &str,
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        if sender != BRIDGE_PLUGIN {
            return Err(format!("only `{BRIDGE_PLUGIN}` may send economy requests"));
        }
        let request: EconomyRequest = decode(message)?;
        Ok(encode(&dispatch(provider, request)))
    }

    fn positive(amount: Amount) -> EconomyResult<Amount> {
        if amount > 0 { Ok(amount) } else { Err(EconomyError::InvalidAmount) }
    }

    fn dispatch(p: &dyn EconomyProvider, request: EconomyRequest) -> EconomyResult<EconomyResponse> {
        use EconomyResponse::*;
        match request {
            EconomyRequest::Balance { account } => p.balance(&account).map(Amount),
            EconomyRequest::Has { account, amount } => p.has(&account, amount).map(Bool),
            EconomyRequest::Deposit { account, amount } => {
                p.deposit(&account, positive(amount)?).map(Amount)
            }
            EconomyRequest::Withdraw { account, amount } => {
                p.withdraw(&account, positive(amount)?).map(Amount)
            }
            EconomyRequest::Transfer { from, to, amount } => {
                p.transfer(&from, &to, positive(amount)?).map(|()| Done)
            }
            EconomyRequest::Format { amount } => Ok(Text(p.format(amount))),
            EconomyRequest::ProviderName => Ok(Text(p.name())),
        }
    }
}
