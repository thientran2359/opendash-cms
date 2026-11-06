param(
    [int]$Port = 18766
)

$ErrorActionPreference = 'Stop'
$url = "http://127.0.0.1:$Port/api/health"
$response = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec 5
if ($response.status -ne 'ok' -or $response.database -ne 'ready' -or $response.scaffold -ne $true) {
    throw "Unexpected health response from $url"
}

Write-Host "PASS: $url (SQLite ready, scaffold true)" -ForegroundColor Green
