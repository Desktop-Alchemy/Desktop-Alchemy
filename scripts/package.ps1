$ErrorActionPreference='Stop'
$root=[System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
& (Join-Path $root 'scripts/build-plugins.ps1') -Release
Push-Location $root
try { pnpm tauri build --bundles nsis; if($LASTEXITCODE -ne 0){throw 'Tauri build failed'} }
finally { Pop-Location }
