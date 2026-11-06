param(
    [Parameter(Mandatory = $false)]
    [string]$TaskText,

    [Parameter(Mandatory = $false)]
    [string]$FromFile,

    [Parameter(Mandatory = $false)]
    [string]$Title = "Task Handoff from Codex"
)

$ErrorActionPreference = 'Stop'

$syncDir = Join-Path $PSScriptRoot 'sync'
if (-not (Test-Path $syncDir)) {
    New-Item -ItemType Directory -Path $syncDir -Force | Out-Null
}

# 1. Clear previous done/result state and reset EventWaitHandle
$donePath = Join-Path $syncDir 'ag-done.json'
$resultPath = Join-Path $syncDir 'result-from-ag.md'
$taskPath = Join-Path $syncDir 'task-for-ag.md'

if (Test-Path $donePath) {
    Remove-Item -Path $donePath -Force -ErrorAction SilentlyContinue
}
if (Test-Path $resultPath) {
    Remove-Item -Path $resultPath -Force -ErrorAction SilentlyContinue
}

try {
    $ev = New-Object System.Threading.EventWaitHandle($false, [System.Threading.EventResetMode]::ManualReset, 'Local\OpenDash_Antigravity_Done')
    [void]$ev.Reset()
    $ev.Dispose()
} catch {
    # Ignore if handle cannot be opened
}

# 2. Read task content from -FromFile, -TaskText, or stdin
$body = ""
if (-not [string]::IsNullOrWhiteSpace($FromFile)) {
    if (-not (Test-Path $FromFile)) {
        throw "Task source file not found: $FromFile"
    }
    $body = Get-Content -Path $FromFile -Raw -Encoding UTF8
} elseif (-not [string]::IsNullOrWhiteSpace($TaskText)) {
    $body = $TaskText
} elseif ([Console]::IsInputRedirected) {
    $body = [Console]::In.ReadToEnd()
} else {
    throw "Provide -TaskText, -FromFile, or pipe task markdown via stdin."
}

$timestamp = (Get-Date).ToString('o')
$markdown = @"
# $Title

- **Prepared At**: `$timestamp`
- **Result File**: `.agents/sync/result-from-ag.md`

---

$($body.Trim())

---

## Yêu cầu báo cáo kết quả (dành cho Antigravity)
Sau khi hoàn thành công việc và chạy các lệnh kiểm chứng cần thiết, hãy ghi tóm tắt ngắn gọn kết quả (các file đã sửa, lệnh đã chạy, kết quả kiểm chứng, giới hạn nếu có) vào `.agents/sync/result-from-ag.md` trước khi kết thúc lượt.
"@

[System.IO.File]::WriteAllText($taskPath, $markdown, [System.Text.Encoding]::UTF8)

$out = [ordered]@{
    status          = 'prepared'
    preparedAt      = $timestamp
    taskFile        = '.agents/sync/task-for-ag.md'
    resultFile      = '.agents/sync/result-from-ag.md'
    webPromptToSend = 'Thực hiện task trong .agents/sync/task-for-ag.md'
    nextCommand     = 'powershell -NoProfile -ExecutionPolicy Bypass -File .\.agents\wait-for-ag.ps1 -TimeoutSeconds 900'
}

$out | ConvertTo-Json -Depth 5
exit 0
