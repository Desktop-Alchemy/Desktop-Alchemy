param([switch]$Release)
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$previousConfig = $env:TAURI_CONFIG
& (Join-Path $PSScriptRoot 'build-sensors.ps1') -Release:$Release
$arguments = @('build', '--manifest-path', (Join-Path $root 'src-tauri/Cargo.toml'), '--bin', 'm18-example-plugin', '--bin', 'm18-icon-publisher', '--bin', 'm18-performance-plugin', '--bin', 'm18-plugin-pack', '--features', 'example-plugin,performance-plugin,diagnostics', '--locked')
if ($Release) { $arguments += '--release' }
try {
    # Bootstrap executables before Tauri validates the resources that contain them.
    $env:TAURI_CONFIG = '{"bundle":{"resources":[]}}'
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Plugin build failed' }
} finally {
    $env:TAURI_CONFIG = $previousConfig
}
$profile = if ($Release) { 'release' } else { 'debug' }
Copy-Item -LiteralPath (Join-Path $root "src-tauri/target/$profile/m18-example-plugin.exe") -Destination (Join-Path $root 'examples/counter/m18-example-plugin.exe')
Copy-Item -LiteralPath (Join-Path $root "src-tauri/target/$profile/m18-performance-plugin.exe") -Destination (Join-Path $root 'plugins/performance/m18-performance-plugin.exe')
# The existing Tauri Windows bundle config consumes this plugin as the `performance.exe` sidecar.
Copy-Item -LiteralPath (Join-Path $root "src-tauri/target/$profile/m18-performance-plugin.exe") -Destination (Join-Path $root "src-tauri/target/$profile/performance.exe") -Force

& (Join-Path $root "src-tauri/target/$profile/m18-plugin-pack.exe") (Join-Path $root 'examples/counter') (Join-Path $root 'examples/counter.m18plugin')
if ($LASTEXITCODE -ne 0) { throw 'Example package failed' }
Copy-Item -LiteralPath (Join-Path $root "src-tauri/target/$profile/m18-icon-publisher.exe") -Destination (Join-Path $root 'examples/icon-publisher/m18-icon-publisher.exe')
foreach ($example in @('icon-pack', 'icon-publisher')) {
    & (Join-Path $root "src-tauri/target/$profile/m18-plugin-pack.exe") (Join-Path $root "examples/$example") (Join-Path $root "examples/$example.m18plugin")
    if ($LASTEXITCODE -ne 0) { throw "$example package failed" }
}
