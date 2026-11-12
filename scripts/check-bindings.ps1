$ErrorActionPreference = 'Stop'
$bindings = Join-Path $PSScriptRoot '../src/bindings.ts'
$expected = [System.IO.File]::ReadAllText($bindings)
& (Join-Path $PSScriptRoot 'test.ps1') -Suite Bindings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$actual = [System.IO.File]::ReadAllText($bindings)
if ($expected -cne $actual) {
    throw 'Generated TypeScript differs from Rust contracts. Run pnpm bindings and include the result.'
}
