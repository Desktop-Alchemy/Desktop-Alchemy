$ErrorActionPreference = 'Stop'
$manifest = Join-Path $PSScriptRoot '../src-tauri/Cargo.toml'
$previousConfig = $env:TAURI_CONFIG
try {
    $env:TAURI_CONFIG = '{"bundle":{"resources":[]}}'
    # One Cargo process at a time, reusing the same target directory.
    foreach ($feature in @('', 'device-m18', 'device-ms-audio-dock')) {
        $arguments = @('check', '--manifest-path', $manifest, '--no-default-features', '--locked')
        if ($feature) { $arguments += @('--features', $feature) }
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    & cargo check --manifest-path $manifest --features device-simulator --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    $env:TAURI_CONFIG = $previousConfig
}
