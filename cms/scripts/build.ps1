param(
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$cmsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$adminRoot = Join-Path $cmsRoot 'admin'

Push-Location $adminRoot
try {
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed ($LASTEXITCODE)" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed ($LASTEXITCODE)" }
}
finally {
    Pop-Location
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

Write-Host 'OpenDash scaffold build complete.' -ForegroundColor Green
