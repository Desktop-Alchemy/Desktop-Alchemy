param([string]$Destination = '.local/example-icon-pack.m18plugin')

$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$source = Join-Path $root 'examples/icon-pack'
$local = Join-Path $root '.local'
$stage = Join-Path $local "pack-icon-example-$([guid]::NewGuid().ToString('N'))"
$license = Join-Path $root 'licenses/Apache-2.0.txt'
if ([System.IO.Path]::IsPathRooted($Destination)) {
    $destinationPath = [System.IO.Path]::GetFullPath($Destination)
} else {
    $destinationPath = [System.IO.Path]::GetFullPath((Join-Path $root $Destination))
}

New-Item -ItemType Directory -Path $local -Force | Out-Null
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {
    Get-ChildItem -LiteralPath $source -Force | Copy-Item -Destination $stage -Recurse -Force
    Copy-Item -LiteralPath $license -Destination (Join-Path $stage 'LICENSE-APACHE-2.0.txt')
    Push-Location $root
    try {
        & cargo run --manifest-path src-tauri/Cargo.toml --bin m18-plugin-pack --features diagnostics --locked -- $stage $destinationPath
        if ($LASTEXITCODE -ne 0) { throw "Icon example packaging failed with exit code $LASTEXITCODE" }
    }
    finally {
        Pop-Location
    }
}
finally {
    $resolvedLocal = [System.IO.Path]::GetFullPath($local).TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    $resolvedStage = [System.IO.Path]::GetFullPath($stage)
    if (!$resolvedStage.StartsWith($resolvedLocal, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove a staging directory outside .local'
    }
    if (Test-Path -LiteralPath $resolvedStage) {
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}
