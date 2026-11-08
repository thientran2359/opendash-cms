$ErrorActionPreference = 'SilentlyContinue'

# Read hook payload from stdin
$rawInput = [Console]::In.ReadToEnd()
$payload = $null
if (-not [string]::IsNullOrWhiteSpace($rawInput)) {
    try {
        $payload = $rawInput | ConvertFrom-Json
    } catch {
        $payload = $null
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$syncDir = Join-Path $PSScriptRoot 'sync'
if (-not (Test-Path $syncDir)) {
    New-Item -ItemType Directory -Path $syncDir -Force | Out-Null
}

$fullyIdle = $true
if ($null -ne $payload -and $null -ne $payload.fullyIdle) {
    $fullyIdle = [bool]$payload.fullyIdle
}

# Only signal completion when Antigravity is fully idle (no running background tasks)
if ($fullyIdle) {
    $resultPath = Join-Path $syncDir 'result-from-ag.md'
    $resultSummary = $null
    if (Test-Path $resultPath) {
        $resultSummary = [System.IO.File]::ReadAllText($resultPath, [System.Text.Encoding]::UTF8)
    }

    # Extract last model response from transcript.jsonl if available
    $lastResponse = $null
    $transcriptPath = if ($payload) { $payload.transcriptPath } else { $null }
    if (-not [string]::IsNullOrWhiteSpace($transcriptPath) -and (Test-Path $transcriptPath)) {
        try {
            $lines = Get-Content -Path $transcriptPath -Tail 25 -Encoding UTF8
            for ($i = $lines.Count - 1; $i -ge 0; $i--) {
                $line = $lines[$i]
                if ([string]::IsNullOrWhiteSpace($line)) { continue }
                $step = $line | ConvertFrom-Json
                if ($step.type -eq 'PLANNER_RESPONSE' -and -not [string]::IsNullOrWhiteSpace($step.content)) {
                    $lastResponse = $step.content
                    break
                }
            }
        } catch {
            # Ignore transcript parse errors
        }
    }

    # If Antigravity didn't explicitly write result-from-ag.md, populate it from lastResponse
    if ([string]::IsNullOrWhiteSpace($resultSummary) -and -not [string]::IsNullOrWhiteSpace($lastResponse)) {
        $resultSummary = $lastResponse
        [System.IO.File]::WriteAllText($resultPath, $lastResponse, [System.Text.Encoding]::UTF8)
    }

    # Capture current git status --short for quick inspection by Codex
    $gitStatus = @()
    try {
        Push-Location $repoRoot
        $gitOut = & git status --short 2>$null
        if ($LASTEXITCODE -eq 0 -and $null -ne $gitOut) {
            $gitStatus = @($gitOut)
        }
    } catch {
    } finally {
        Pop-Location
    }

    $statusObj = [ordered]@{
        status            = 'done'
        completedAt       = (Get-Date).ToString('o')
        conversationId    = if ($payload) { $payload.conversationId } else { $null }
        terminationReason = if ($payload) { $payload.terminationReason } else { 'UNKNOWN' }
        fullyIdle         = $fullyIdle
        error             = if ($payload) { $payload.error } else { '' }
        resultFile        = if (Test-Path $resultPath) { '.agents/sync/result-from-ag.md' } else { $null }
        resultSummary     = $resultSummary
        lastResponse      = $lastResponse
        gitStatusShort    = $gitStatus
        transcriptPath    = $transcriptPath
    }

    $json = $statusObj | ConvertTo-Json -Depth 6

    # Write atomically via temp file + move so FileSystemWatcher sees a clean Rename/Write
    $tmpPath = Join-Path $syncDir 'ag-done.tmp'
    $donePath = Join-Path $syncDir 'ag-done.json'
    [System.IO.File]::WriteAllText($tmpPath, $json, [System.Text.Encoding]::UTF8)
    Move-Item -Path $tmpPath -Destination $donePath -Force

    # Signal Named EventWaitHandle to wake up wait-for-ag.ps1 immediately (0ms latency)
    try {
        $ev = New-Object System.Threading.EventWaitHandle($false, [System.Threading.EventResetMode]::ManualReset, 'Local\OpenDash_Antigravity_Done')
        [void]$ev.Set()
        $ev.Dispose()
    } catch {
        # Ignore if handle cannot be opened
    }
}

# Return valid JSON decision to allow Antigravity to stop normally
[Console]::Out.WriteLine('{"decision":"stop"}')
exit 0
