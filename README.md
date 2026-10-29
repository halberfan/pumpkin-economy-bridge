# Pumpkin Economy Bridge

A bridge between economy plugins and plugins that use money, for the
[Pumpkin](https://pumpkinmc.org) Minecraft server's WebAssembly plugin API.

Plugins that need money (shops, cities, jobs, ...) don't talk to an economy
plugin directly. They talk to the bridge, and the bridge forwards every call to
whichever economy plugin is installed. Server owners can switch economy plugins
without breaking anything.

```text
your plugin --> economy_bridge --> economy plugin (provider)
```

This repository contains:

| Crate            | What it is                                                                 |
| ---------------- | -------------------------------------------------------------------------- |
| `economy-bridge` | The plugin. Put `economy_bridge.wasm` into the server's `plugins` folder. |
| `economy-api`    | Rust library for plugin developers, both consumers and providers.         |

Requires `pumpkin-plugin-api` 0.2.1.

## Installing on a server

1. Download `economy_bridge.wasm` from the releases page, or build it (see below).
2. Put it into `plugins/` together with an economy plugin that supports the bridge.

## Using money in your plugin

Add the library to your `Cargo.toml`:

```toml
[dependencies]
economy-api = { git = "https://github.com/halberfan/pumpkin-economy-bridge", tag = "v0.1.0" }
```

Declare the bridge as a dependency in your plugin metadata, then use
`economy_api::client`:

```rust
use economy_api::{EconomyError, account_of, client};

// in PluginMetadata
dependencies: vec![economy_api::BRIDGE_PLUGIN.into()],

// somewhere in a command handler
let account = account_of(&player);
match client::withdraw(&account, 50_00) {
    Ok(remaining) => { /* charged 50.00 */ }
    Err(EconomyError::InsufficientFunds { balance }) => { /* not enough money */ }
    Err(e) => { /* no economy installed, IPC failure, ... */ }
}
```

Available calls: `balance`, `has`, `deposit`, `withdraw`, `transfer`, `format`,
`provider_name`, `is_available`.

Amounts are `i64` in the currency's smallest unit (e.g. cents), so `50_00` is
50.00. Use `client::format` to show amounts in the provider's currency.
Accounts are identified by the player's UUID string, which `account_of` gives
you.

## Writing an economy plugin

Implement `EconomyProvider`, register in `on_load` and pass IPC messages on:

```rust
use economy_api::{Amount, EconomyResult, provider::{self, EconomyProvider}};

struct MyEconomy;

impl EconomyProvider for MyEconomy {
    fn name(&self) -> String { "my_economy".into() }
    fn balance(&self, account: &str) -> EconomyResult<Amount> { todo!() }
    fn deposit(&self, account: &str, amount: Amount) -> EconomyResult<Amount> { todo!() }
    fn withdraw(&self, account: &str, amount: Amount) -> EconomyResult<Amount> { todo!() }
    fn format(&self, amount: Amount) -> String { todo!() }
}

impl Plugin for MyEconomy {
    // metadata: dependencies: vec![economy_api::BRIDGE_PLUGIN.into()]

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        provider::register().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
        let _ = provider::unregister();
        Ok(())
    }

    fn handle_ipc_message(&self, sender: String, message: Vec<u8>) -> Result<Vec<u8>, String> {
        provider::handle_message(self, &sender, &message)
    }
}
```

`deposit` and `withdraw` return the new balance. Amounts are checked to be
positive before they reach your provider. `has` and `transfer` have default
implementations you can override.

Only one provider can be registered at a time. A second one gets
`EconomyError::ProviderAlreadyRegistered`.

## Protocol

Everything is JSON over Pumpkin's plugin IPC, so you can also implement it
without this library. See [`economy-api/src/lib.rs`](economy-api/src/lib.rs) for the
message types.

- Consumers and providers send a `BridgeMessage` to the plugin `economy_bridge`.
- The bridge sends an `EconomyRequest` to the provider.
- Every reply is a `Result<EconomyResponse, EconomyError>`.

## Building

You need Rust with the `wasm32-wasip2` target and clang:

```sh
rustup target add wasm32-wasip2
cargo build --release
```

The plugin is written to `target/wasm32-wasip2/release/economy_bridge.wasm`.
