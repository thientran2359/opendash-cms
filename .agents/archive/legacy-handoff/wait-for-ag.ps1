param(
    [switch]$Clear,
    [int]$TimeoutSeconds = 900
)

$ErrorActionPreference = 'Stop'
$syncDir = Join-Path $PSScriptRoot 'sync'
if (-not (Test-Path $syncDir)) {
    New-Item -ItemType Directory -Path $syncDir -Force | Out-Null
}

$donePath = Join-Path $syncDir 'ag-done.json'
$eventName = 'Local\OpenDash_Antigravity_Done'

$ev = New-Object System.Threading.EventWaitHandle($false, [System.Threading.EventResetMode]::ManualReset, $eventName)

try {
    if ($Clear) {
        if (Test-Path $donePath) {
            Remove-Item -Path $donePath -Force -ErrorAction SilentlyContinue
        }
        [void]$ev.Reset()
        Write-Output '{"status":"cleared"}'
        exit 0
    }

    # Fast path: if ag-done.json already exists before waiting
    if (Test-Path $donePath) {
        Get-Content -Path $donePath -Raw -Encoding UTF8
        exit 0
    }

    # Also watch the sync directory as a fallback in case the EventWaitHandle wasn't caught
    $watcher = New-Object System.IO.FileSystemWatcher($syncDir, 'ag-done.json')
    $watcher.NotifyFilter = [System.IO.NotifyFilters]::FileName -bor [System.IO.NotifyFilters]::LastWrite
    $watcher.EnableRaisingEvents = $true

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)

    while ((Get-Date) -lt $deadline) {
        if (Test-Path $donePath) {
            Start-Sleep -Milliseconds 50
            Get-Content -Path $donePath -Raw -Encoding UTF8
            $watcher.Dispose()
            exit 0
        }

        # Block on kernel EventWaitHandle in 1-second slices (0% CPU spinning, instant wakeup on signal)
        if ($ev.WaitOne(1000)) {
            Start-Sleep -Milliseconds 50
            if (Test-Path $donePath) {
                Get-Content -Path $donePath -Raw -Encoding UTF8
                $watcher.Dispose()
                exit 0
            }
        }
    }

    $watcher.Dispose()
    Write-Output ('{"status":"timeout","timeoutSeconds":' + $TimeoutSeconds + '}')
    exit 124
} finally {
    $ev.Dispose()
}
