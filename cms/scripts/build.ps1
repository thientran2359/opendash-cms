param(
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$cmsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach ($package in 'admin', 'examples/plugins/sample-plugin') {
    $packageRoot = Join-Path $cmsRoot $package
    if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'package-lock.json'))) {
        throw "$package/package-lock.json missing"
    }
    Push-Location $packageRoot
    try {
        npm ci
        if ($LASTEXITCODE -ne 0) { throw "$package npm ci failed ($LASTEXITCODE)" }
        npm run build
        if ($LASTEXITCODE -ne 0) { throw "$package build failed ($LASTEXITCODE)" }
    }
    finally {
        Pop-Location
    }
}

Push-Location $cmsRoot
try {
    $cargoArgs = @('build', '--locked', '-p', 'opendash-server')
    if ($Release) { $cargoArgs += '--release' }
    cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
}
finally {
    Pop-Location
}

Write-Host 'OpenDash build complete.' -ForegroundColor Green
