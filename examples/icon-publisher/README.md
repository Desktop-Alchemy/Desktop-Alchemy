# Runtime Icon Publishing Example

`src-tauri/src/bin/icon_publisher.rs` calls JSON-RPC `icons.publish`, `icons.list`, and `icons.remove`, then republishes a transparent play icon. Published icons remain available after stopping/disabling the process and are removed on uninstall.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin m18-icon-publisher --features example-plugin --release --locked
Copy-Item src-tauri/target/release/m18-icon-publisher.exe examples/icon-publisher/
pnpm plugin:pack -- examples/icon-publisher .local/example-icon-publisher.m18plugin
```

Only icon API permissions are used; the example makes no device calls. Glyphs are Apache-2.0 licensed and have been resized/recolored. The example retains its license file.
