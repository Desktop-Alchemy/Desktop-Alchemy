$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$vendor = Join-Path $root 'vendor/device-sdk'
$native = Join-Path $vendor 'native'
$licenses = Join-Path $root 'licenses'
$manifest = Get-Content -LiteralPath (Join-Path $native 'SOURCES.json') -Raw | ConvertFrom-Json
if ($manifest.commit -notmatch '^[0-9a-f]{40}$') { throw 'Invalid pinned SDK commit' }
$base = "https://raw.githubusercontent.com/MiraboxSpace/StreamDock-Device-SDK/$($manifest.commit)"

foreach ($file in $manifest.files) {
  if ($file.name -notmatch '^[A-Za-z0-9_]+\.dll$' -or $file.sha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'Invalid native SDK file manifest'
  }
  $destination = Join-Path $native $file.name
  $download = "$destination.download"
  try {
    Invoke-WebRequest "$base/$($manifest.directory)/$($file.name)" -OutFile $download
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
      $hash = [System.BitConverter]::ToString($sha256.ComputeHash([System.IO.File]::ReadAllBytes($download))).Replace('-', '').ToLowerInvariant()
    }
    finally { $sha256.Dispose() }
    if ($hash -ne $file.sha256 -or (Get-Item -LiteralPath $download).Length -ne $file.bytes) {
      throw "C SDK file does not match pinned manifest: $($file.name)"
    }
    Move-Item -LiteralPath $download -Destination $destination -Force
  }
  finally {
    if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download }
  }
}
Invoke-WebRequest "$base/LICENSE" -OutFile (Join-Path $licenses 'device-sdk-MIT.txt')
Invoke-WebRequest "$base/CPP-SDK/LICENSE" -OutFile (Join-Path $licenses 'device-sdk-CPP-SDK-MIT-and-notice.txt')
Write-Output "Official C SDK pinned: $($manifest.commit); $($manifest.files.Count) native DLLs verified"
