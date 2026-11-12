param(
    [ValidateSet('Core', 'Mcp', 'Plugins', 'Launch', 'Full', 'Bindings')]
    [string]$Suite = 'Core',
    [string]$Filter
)
$ErrorActionPreference = 'Stop'
$manifest = Join-Path $PSScriptRoot '../src-tauri/Cargo.toml'
$arguments = @('test', '--manifest-path', $manifest, '--locked')
switch ($Suite) {
    'Core' { $arguments += @('--workspace', '--lib') }
    'Mcp' { $arguments += @('--lib', 'mcp::tests') }
    'Plugins' { $arguments += @('--test', 'plugins', '--features', 'diagnostics,example-plugin,performance-plugin') }
    'Launch' { $arguments += @('--test', 'program_launch', '--features', 'diagnostics') }
    'Full' { $arguments += @('--all-targets', '--all-features') }
    'Bindings' { $arguments += @('--lib', 'bindings::export_types') }
}
if ($Filter) {
    if ($Suite -in @('Mcp', 'Bindings')) { throw "Suite $Suite already specifies its filter" }
    $arguments += $Filter
}
if ($Suite -eq 'Core') { $arguments += @('--', '--skip', 'mcp::tests') }
if ($Suite -eq 'Bindings') { $arguments += @('--', '--ignored', '--exact') }

# Tests never load device DLLs or need packaged plugins/.NET output.
# Restore the caller's config; packaging continues to validate real resources.
$previousConfig = $env:TAURI_CONFIG
try {
    $env:TAURI_CONFIG = '{"bundle":{"resources":[]}}'
    & cargo @arguments
    $result = $LASTEXITCODE
} finally {
    $env:TAURI_CONFIG = $previousConfig
}
exit $result
