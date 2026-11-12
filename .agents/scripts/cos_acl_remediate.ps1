# CoS Windows W01: owner-scoped ACL correction with a private recovery snapshot.
# Modes: Audit (read-only), SelfTest (temporary fixture), Apply (live CoS).
# Live rollback requires a separately reviewed recovery procedure; no silent reset.
# Requires the owner account; never restarts CoS, rotates tokens or reads chats.
param(
    [ValidateSet('Audit', 'SelfTest', 'Apply')]
    [string]$Mode = 'Audit',
    [string]$BackupId = ''
)

$ErrorActionPreference = 'Stop'
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$currentSid = $identity.User.Value
$trusted = @($currentSid, 'S-1-5-18', 'S-1-5-32-544', 'S-1-3-0', 'S-1-3-4')
$appData = [Environment]::GetFolderPath('ApplicationData')
$base = Join-Path $appData 'chat-on-steroids'
$adapterRoot = Join-Path $base 'atlas-dispatch'
$backupBase = Join-Path $adapterRoot 'w01-acl-backup'
$paths = @(
    $base,
    (Join-Path $base 'control-api'),
    (Join-Path $base 'control-api\token'),
    (Join-Path $base 'control-api\endpoint.json'),
    (Join-Path $base 'state'),
    (Join-Path $base 'state\projects.json')
)

function Invoke-Icacls([string[]]$Arguments) {
    & icacls.exe @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'ICACLS_OPERATION_FAILED' }
}

function Get-TrustSnapshot([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { throw 'MISSING_REQUIRED_OBJECT' }
    $item = Get-Item -LiteralPath $Path
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'REPARSE_POINT_NOT_ALLOWED'
    }
    $acl = Get-Acl -LiteralPath $Path
    $owner = if ($acl.Owner -match '^S-1-') { $acl.Owner } else {
        ([Security.Principal.NTAccount]::new($acl.Owner)).Translate(
            [Security.Principal.SecurityIdentifier]).Value
    }
    if ($owner -ne $currentSid) { throw 'OWNER_MISMATCH' }
    $rules = @($acl.GetAccessRules($true, $true,
                   [Security.Principal.SecurityIdentifier]))
    $unexpected = @($rules | Where-Object {
        $_.AccessControlType -eq 'Allow' -and
        $trusted -notcontains $_.IdentityReference.Value -and
        (([int]$_.FileSystemRights -band 0x000D0157) -ne 0)
    })
    return [pscustomobject]@{
        Protected = $acl.AreAccessRulesProtected
        Unexpected = $unexpected.Count
        UnexpectedExplicit = @($unexpected | Where-Object { -not $_.IsInherited }).Count
    }
}

function Lock-Acl([string]$Path, [bool]$Directory) {
    $before = Get-TrustSnapshot $Path
    if ($before.UnexpectedExplicit -ne 0) {
        throw 'UNEXPECTED_EXPLICIT_ACL_REQUIRES_MANUAL_REVIEW'
    }
    if ($before.Protected -and $before.Unexpected -eq 0) {
        # Already secured in a previous step; preserve this ACL untouched.
        return
    }
    $suffix = if ($Directory) { ':(OI)(CI)F' } else { ':F' }
    Invoke-Icacls @($Path, '/grant:r', ('*' + $currentSid + $suffix),
                    ('*S-1-5-18' + $suffix), ('*S-1-5-32-544' + $suffix))
    Invoke-Icacls @($Path, '/inheritance:r')
    $after = Get-TrustSnapshot $Path
    if ($after.Unexpected -ne 0 -or -not $after.Protected) {
        throw 'ACL_HARDEN_VALIDATION_FAILED'
    }
}

function Restore-InheritedAcl([string[]]$Directories, [string[]]$Files) {
    # Fixture-only helper. Never invoke on live CoS paths: icacls /reset is
    # inheritance-based, not a precise restore of a captured SDDL snapshot.
    foreach ($item in @($Directories) + @($Files)) {
        Get-TrustSnapshot $item | Out-Null
    }
    foreach ($dir in $Directories) { Invoke-Icacls @($dir, '/reset') }
    foreach ($file in $Files) { Invoke-Icacls @($file, '/reset') }
}

if ($Mode -eq 'Audit') {
    $observations = @(foreach ($path in $paths) {
        $snapshot = Get-TrustSnapshot $path
        [pscustomobject]@{ Kind = $(if ((Get-Item -LiteralPath $path).PSIsContainer) {
                'directory'
            } else { 'file' }); Protected = $snapshot.Protected;
            UnexpectedAllow = $snapshot.Unexpected }
    })
    [pscustomobject]@{ Mode = 'Audit'; Objects = $observations.Count;
        UnexpectedAllowByObject = @($observations | ForEach-Object { $_.UnexpectedAllow })
    } | ConvertTo-Json -Compress -Depth 4
    exit 0
}

if ($Mode -eq 'SelfTest') {
    $temp = Join-Path ([IO.Path]::GetTempPath()) ('opendash-w01-test-' +
        [guid]::NewGuid().ToString('N'))
    $child = Join-Path $temp 'private'
    try {
        New-Item -ItemType Directory -Path $temp -Force | Out-Null
        Invoke-Icacls @($temp, '/grant', '*S-1-5-32-545:(OI)(CI)M')
        New-Item -ItemType Directory -Path $child | Out-Null
        $file = Join-Path $child 'fixture.json'
        Set-Content -LiteralPath $file -Value 'fixture' -Encoding UTF8
        if ((Get-TrustSnapshot $child).Unexpected -eq 0) {
            throw 'SELFTEST_FAILED_TO_BUILD_UNTRUSTED_FIXTURE'
        }
        Lock-Acl $child $true
        Lock-Acl $file $false
        if ((Get-TrustSnapshot $file).Unexpected -ne 0) {
            throw 'SELFTEST_FILE_STILL_INSECURE'
        }
        # Validate the rollback mechanism on disposable ACLs, never by
        # weakening the live CoS files during an active session.
        Restore-InheritedAcl @($child) @($file)
        if ((Get-TrustSnapshot $child).Unexpected -eq 0) {
            throw 'SELFTEST_ROLLBACK_DID_NOT_RESTORE_ACL'
        }
        '{"mode":"SelfTest","result":"PASS"}'
    } finally {
        if (Test-Path -LiteralPath $temp) {
            Remove-Item -LiteralPath $temp -Recurse -Force
        }
    }
    exit 0
}

if ($Mode -eq 'Apply') {
    foreach ($path in $paths) {
        $snapshot = Get-TrustSnapshot $path
        if ($snapshot.UnexpectedExplicit -ne 0) {
            throw 'EXPLICIT_GRANTS_REQUIRE_REVIEW'
        }
        # Never silently discard pre-existing custom access rules during
        # rollback. The CoS W01 initial snapshot had inherited rules only.
        $existing = Get-Acl -LiteralPath $path
        if (-not $existing.AreAccessRulesProtected -and
            @($existing.GetAccessRules($true, $true,
                [Security.Principal.SecurityIdentifier]) | Where-Object {
                    -not $_.IsInherited
                }).Count -ne 0) {
            throw 'NONINHERITED_BASELINE_REQUIRES_MANUAL_REVIEW'
        }
    }
    if (-not (Test-Path -LiteralPath $adapterRoot)) {
        throw 'BOOTSTRAP_ADAPTER_FIRST'
    }
    $stamp = 'w01-' + (Get-Date).ToString('yyyyMMddTHHmmss') + '-' +
        [guid]::NewGuid().ToString('N').Substring(0, 8)
    $folder = Join-Path $backupBase $stamp
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    # Protect the backups under the adapter's restricted ACL, as these contain SIDs.
    Invoke-Icacls @($base, '/save', (Join-Path $folder 'cos-root.acl'))
    Invoke-Icacls @((Join-Path $base 'control-api'), '/save',
                    (Join-Path $folder 'control-api.acl'), '/t')
    Invoke-Icacls @((Join-Path $base 'state'), '/save',
                    (Join-Path $folder 'state.acl'), '/t')
    try {
        foreach ($path in $paths) {
            Lock-Acl $path ([bool](Get-Item -LiteralPath $path).PSIsContainer)
        }
        foreach ($path in $paths) {
            if ((Get-TrustSnapshot $path).Unexpected -ne 0) {
                throw 'POST_APPLY_ACL_INSECURE'
            }
        }
        [pscustomobject]@{ Mode = 'Apply'; Result = 'PASS'; BackupId = $stamp;
            Objects = $paths.Count } | ConvertTo-Json -Compress
    } catch {
        # Retain any successfully secured objects and the private ACL backup.
        # Do not reset an evolving live CoS file to an older insecure parent.
        throw 'HARDEN_INCOMPLETE_REVIEW_PRIVATE_BACKUP_AND_AUDIT'
    }
}
