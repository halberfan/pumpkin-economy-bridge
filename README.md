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

Everything is UTF-8 JSON sent with Pumpkin's `send-ipc-message` and received in
`handle-ipc-message`. You don't need the Rust library, so plugins in other
languages (Python, JavaScript, Go, ...) can use the bridge or be a provider too.
The types are defined in [`economy-api/src/lib.rs`](economy-api/src/lib.rs).

### Conventions

- **Accounts** are player UUID strings in the standard hyphenated form, e.g.
  `"069a79f4-44e9-4726-a5be-fca90e38aaf5"`.
- **Amounts** are integers in the currency's smallest unit. `5000` is 50.00
  with two decimals. Never send floats.
- **Enums** use serde's default externally tagged format:
  - a variant without data is a plain string: `"RegisterProvider"`
  - a variant with fields is an object with one key: `{"Balance": {"account": "..."}}`
  - a variant with one value is an object with one key: `{"Amount": 5000}`
- **Results** are `{"Ok": <value>}` or `{"Err": <error>}`.

### Flow

```text
consumer  --BridgeMessage-->  economy_bridge  --EconomyRequest-->  provider
          <--Result---------                  <--Result---------
```

| Who sends   | To               | Message          | Reply                                     |
| ----------- | ---------------- | ---------------- | ----------------------------------------- |
| Consumer    | `economy_bridge` | `BridgeMessage`  | `Result<EconomyResponse, EconomyError>`   |
| Provider    | `economy_bridge` | `BridgeMessage`  | `Result<EconomyResponse, EconomyError>`   |
| Bridge      | provider         | `EconomyRequest` | `Result<EconomyResponse, EconomyError>`   |

### `BridgeMessage` (sent to `economy_bridge`)

| Message                         | Meaning                                                        | Ok reply |
| ------------------------------- | -------------------------------------------------------------- | -------- |
| `"RegisterProvider"`            | Registers the sending plugin as the economy provider.          | `"Done"` |
| `"UnregisterProvider"`          | Removes the sending plugin as provider (ignored otherwise).    | `"Done"` |
| `{"Call": <EconomyRequest>}`    | Forwards the request to the provider and returns its reply.    | depends on the request |

The bridge identifies the provider by the IPC sender, so a plugin can only
register itself.

### `EconomyRequest` (inside `Call`, and sent by the bridge to the provider)

| Request                                                        | Ok reply                |
| -------------------------------------------------------------- | ----------------------- |
| `{"Balance": {"account": "<uuid>"}}`                           | `{"Amount": <balance>}` |
| `{"Has": {"account": "<uuid>", "amount": 5000}}`               | `{"Bool": true}`        |
| `{"Deposit": {"account": "<uuid>", "amount": 5000}}`           | `{"Amount": <new balance>}` |
| `{"Withdraw": {"account": "<uuid>", "amount": 5000}}`          | `{"Amount": <new balance>}` |
| `{"Transfer": {"from": "<uuid>", "to": "<uuid>", "amount": 5000}}` | `"Done"`            |
| `{"Format": {"amount": 5000}}`                                 | `{"Text": "50.00 Coins"}` |
| `"ProviderName"`                                               | `{"Text": "simple_economy"}` |

Providers must reject `Deposit`, `Withdraw` and `Transfer` with an amount of
zero or less with `"InvalidAmount"`. Unknown accounts should be created on first
use.

### `EconomyResponse`

| Value              | Used for                                  |
| ------------------ | ----------------------------------------- |
| `{"Amount": 5000}` | `Balance`, `Deposit`, `Withdraw`          |
| `{"Bool": true}`   | `Has`                                     |
| `{"Text": "..."}`  | `Format`, `ProviderName`                  |
| `"Done"`           | `Transfer`, `RegisterProvider`, `UnregisterProvider` |

### `EconomyError`

| Value                                         | Meaning                                         |
| --------------------------------------------- | ----------------------------------------------- |
| `"NoProvider"`                                | No economy plugin is registered.                |
| `{"ProviderAlreadyRegistered": "<plugin>"}`   | Another plugin is already the provider.         |
| `{"InsufficientFunds": {"balance": 1200}}`    | Not enough money; contains the current balance. |
| `"InvalidAmount"`                             | Amount was zero or negative.                    |
| `{"Transport": "<message>"}`                  | IPC failed or a message could not be decoded.   |
| `{"Provider": "<message>"}`                   | Any other provider-specific failure.            |

### Examples

A shop withdraws 50.00 from a player:

```json
-> economy_bridge   {"Call": {"Withdraw": {"account": "069a79f4-44e9-4726-a5be-fca90e38aaf5", "amount": 5000}}}
<- economy_bridge   {"Ok": {"Amount": 4950}}
```

The player doesn't have enough money:

```json
<- economy_bridge   {"Err": {"InsufficientFunds": {"balance": 1200}}}
```

No economy plugin is installed:

```json
<- economy_bridge   {"Err": "NoProvider"}
```

### Notes for providers

- Only answer requests whose IPC sender is `economy_bridge`.
- Always reply with a JSON `Result`, also for errors like `InsufficientFunds`.
  Use the IPC-level error string of `handle-ipc-message` only when the message
  itself cannot be decoded.
- Calls are synchronous. Don't hold a lock while calling the bridge, since the
  call can come back into your plugin before it returns.

## Building

You need Rust with the `wasm32-wasip2` target and clang:

```sh
rustup target add wasm32-wasip2
cargo build --release
```

The plugin is written to `target/wasm32-wasip2/release/economy_bridge.wasm`.
