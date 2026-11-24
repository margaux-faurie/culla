<#
.SYNOPSIS
  Create, mount, detach, lock and unmount VHDX test volumes for Windows CI.

.DESCRIPTION
  See scripts/ci/README.md and docs/testing.md section 5.1. Needs an elevated
  shell (GitHub-hosted Windows runners are). Every native call (diskpart,
  mountvol, fsutil) runs with a timeout, so a blocked detach cannot hang the job.

  Actions:
    mount     Create a new expandable VHDX at -Path (refuses an existing file),
              attach it, create one primary partition, quick-format it with
              -FileSystem and assign -Letter.
    attach    Attach an existing, detached VHDX and put its volume on -Letter
              (moving it off any other letter). Used to reattach after a detach.
    detach    Make the volume vanish while processes may hold handles on it.
              -Method auto (default) tries diskpart, then mountvol, then
              Dismount-DiskImage, and stops at the first after which the volume
              root is gone.
    readonly  Simulate a locked SD card. -Method volume (default): diskpart
              'attributes volume set readonly' + fsutil dismount; disk: diskpart
              'attributes disk set readonly' + fsutil dismount; vdisk: detach and
              'attach vdisk readonly'.
    unmount   Detach the VHDX if it is attached. The .vhdx file is left in place.

  With -ResultFile, one line 'action=... outcome=ok|failed method=... ...' is
  appended to that file. Exit code 0 on success, 1 on failure.

.EXAMPLE
  pwsh -File scripts/ci/vhdx.ps1 -Action mount -Path $env:RUNNER_TEMP\t.vhdx -SizeMB 256 -FileSystem exFAT -Letter T
  pwsh -File scripts/ci/vhdx.ps1 -Action detach -Path $env:RUNNER_TEMP\t.vhdx -Letter T -ResultFile r.txt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('mount', 'attach', 'detach', 'readonly', 'unmount')]
    [string] $Action,

    [Parameter(Mandatory)]
    [string] $Path,

    [ValidateRange(16, 4096)]
    [int] $SizeMB = 256,

    [ValidateSet('exFAT', 'FAT32', 'NTFS')]
    [string] $FileSystem = 'exFAT',

    [ValidatePattern('^[A-Za-z]$')]
    [string] $Letter,

    [ValidateSet('auto', 'diskpart', 'mountvol', 'dismountdiskimage', 'volume', 'disk', 'vdisk')]
    [string] $Method,

    [ValidateRange(5, 600)]
    [int] $TimeoutSec = 60,

    [string] $ResultFile
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

function Write-Step([string] $Message) {
    Write-Host "vhdx: $Message"
}

function Write-Result([hashtable] $Fields) {
    $line = ($Fields.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ' '
    Write-Host "vhdx result: $line"
    if ($ResultFile) { Add-Content -LiteralPath $ResultFile -Value $line -Encoding utf8 }
}

function Get-Root([string] $L) { "$($L.ToUpperInvariant()):\" }

function Test-VolumePresent([string] $L) {
    # .NET rather than Test-Path: PowerShell drive caching must not mask a vanished volume.
    [System.IO.Directory]::Exists((Get-Root $L))
}

function Wait-Volume([string] $L, [bool] $Present, [int] $Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if ((Test-VolumePresent $L) -eq $Present) { return $true }
        Start-Sleep -Milliseconds 250
    }
    return ((Test-VolumePresent $L) -eq $Present)
}

# Runs a native program with a timeout. Never throws on a non-zero exit code;
# the caller decides. A timed-out process tree is killed.
function Invoke-Native([string] $FilePath, [string] $Arguments, [int] $Timeout) {
    $out = [System.IO.Path]::GetTempFileName()
    $err = [System.IO.Path]::GetTempFileName()
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $FilePath -ArgumentList $Arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
    $null = $p.Handle # caches the handle so ExitCode is readable after exit
    $exited = $p.WaitForExit($Timeout * 1000)
    if (-not $exited) {
        Write-Step "$FilePath $Arguments did not finish in $Timeout s; killing it"
        & taskkill.exe /T /F /PID $p.Id 2>&1 | Out-Host
        $null = $p.WaitForExit(10000)
    } else {
        $p.WaitForExit() # flushes redirected output
    }
    $watch.Stop()
    $text = ((Get-Content -LiteralPath $out -Raw -ErrorAction SilentlyContinue), (Get-Content -LiteralPath $err -Raw -ErrorAction SilentlyContinue)) -join ''
    Remove-Item -LiteralPath $out, $err -ErrorAction SilentlyContinue
    if ($text) { Write-Host $text.TrimEnd() }
    [pscustomobject]@{
        ExitCode = if ($exited) { $p.ExitCode } else { $null }
        TimedOut = -not $exited
        Seconds  = [math]::Round($watch.Elapsed.TotalSeconds, 1)
        Text     = [string] $text
        Ok       = $exited -and $p.ExitCode -eq 0
        Status   = if (-not $exited) { 'timeout' } elseif ($p.ExitCode -eq 0) { 'ok' } else { "exit$($p.ExitCode)" }
    }
}

function Invoke-Diskpart([string[]] $Lines, [int] $Timeout = $TimeoutSec) {
    $script = [System.IO.Path]::GetTempFileName()
    Set-Content -LiteralPath $script -Value $Lines -Encoding ascii
    Write-Step ("diskpart: " + ($Lines -join ' ; '))
    try {
        return Invoke-Native 'diskpart.exe' "/s `"$script`"" $Timeout
    } finally {
        Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
    }
}

function Assert-Native($Result, [string] $What) {
    if (-not $Result.Ok) { throw "$What failed ($($Result.Status))" }
}

function Get-VhdImage {
    Get-DiskImage -ImagePath $script:FullPath -ErrorAction Stop
}

function Get-VhdPartition {
    $image = Get-VhdImage
    if (-not $image.Attached) { throw "$script:FullPath is not attached" }
    $disk = $image | Get-Disk
    $parts = @($disk | Get-Partition | Where-Object { $_.Type -ne 'Reserved' -and $_.Size -gt 0 })
    if ($parts.Count -ne 1) { throw "expected one data partition on disk $($disk.Number), found $($parts.Count)" }
    $parts[0]
}

# Refuses the system drive and any letter that no partition of the VHDX at -Path holds
# right now. Guards detach, readonly and every mountvol /p, so a wrong -Letter can
# never take a real volume offline or make it read-only.
function Assert-LetterIsVhdx([string] $L) {
    $L = $L.ToUpperInvariant()
    $system = ([string] $env:SystemDrive).TrimEnd(':', '\').ToUpperInvariant()
    if (-not $system) { $system = 'C' }
    if ($L -eq $system) { throw "refusing to act on the system drive ${L}:" }
    $image = Get-VhdImage
    if (-not $image.Attached) { throw "$script:FullPath is not attached, so ${L}: is not one of its volumes" }
    $letters = @($image | Get-Disk | Get-Partition | Where-Object { Test-HasLetter $_ } |
        ForEach-Object { ([string] $_.DriveLetter).ToUpperInvariant() })
    if ($L -notin $letters) {
        throw "drive ${L}: is not backed by $script:FullPath (its letters: $(if ($letters) { $letters -join ', ' } else { 'none' })); refusing"
    }
}

function Invoke-DismountDiskImage {
    $cmd = "`$ErrorActionPreference='Stop'; Dismount-DiskImage -ImagePath '$($script:FullPath)' | Out-Null"
    Invoke-Native 'powershell.exe' "-NoProfile -NonInteractive -Command `"$cmd`"" $TimeoutSec
}

function Test-HasLetter($Part) {
    $c = [string] $Part.DriveLetter
    return ($c.Length -eq 1 -and $c -ne "`0" -and $c -match '^[A-Za-z]$')
}

# Puts the VHDX's single volume on $L, moving it off another letter if needed.
function Set-VolumeLetter([string] $L) {
    $L = $L.ToUpperInvariant()
    $deadline = (Get-Date).AddSeconds(30)
    $part = $null
    while (-not $part -and (Get-Date) -lt $deadline) {
        try { $part = Get-VhdPartition } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $part) { $part = Get-VhdPartition } # surfaces the real error
    # The mount manager may still be giving the volume its remembered letter.
    $letterDeadline = (Get-Date).AddSeconds(10)
    while (-not (Test-HasLetter $part) -and (Get-Date) -lt $letterDeadline) {
        Start-Sleep -Milliseconds 500
        $part = Get-VhdPartition
    }
    $current = if (Test-HasLetter $part) { ([string] $part.DriveLetter).ToUpperInvariant() } else { '' }
    if ($current -eq $L -and (Wait-Volume $L $true 15)) { return }
    if ($current -and $current -ne $L) {
        Write-Step "volume came back on ${current}:, moving it to ${L}:"
        $r = Invoke-Native 'mountvol.exe' "${current}: /d" 30
        Assert-Native $r "mountvol ${current}: /d"
    }
    $guid = @($part.AccessPaths | Where-Object { $_ -like '\\?\Volume{*' })[0]
    if (-not $guid) { throw "no volume GUID path for partition $($part.PartitionNumber) on disk $($part.DiskNumber)" }
    $r = Invoke-Native 'mountvol.exe' "${L}: $guid" 30
    Assert-Native $r "mountvol ${L}: $guid"
    if (-not (Wait-Volume $L $true 30)) { throw "volume did not appear on ${L}:" }
}

function Invoke-Mount {
    if (-not $Letter) { throw '-Letter is required for mount' }
    if (Test-Path -LiteralPath $script:FullPath) { throw "$script:FullPath already exists; mount never reuses or replaces a file" }
    if (Test-VolumePresent $Letter) { throw "drive ${Letter}: is already in use" }
    if ($FileSystem -eq 'FAT32' -and $SizeMB -lt 64) { throw 'FAT32 needs at least 64 MB here' }
    $dir = Split-Path -Parent $script:FullPath
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }

    $label = "CULLA_$($Letter.ToUpperInvariant())" # 7 chars: fits the 11-char FAT/exFAT limit
    $r = Invoke-Diskpart @(
        "create vdisk file=`"$script:FullPath`" maximum=$SizeMB type=expandable",
        "select vdisk file=`"$script:FullPath`"",
        'attach vdisk',
        'create partition primary',
        "format fs=$($FileSystem.ToLowerInvariant()) quick label=$label",
        "assign letter=$($Letter.ToUpperInvariant())"
    ) 180
    Assert-Native $r 'diskpart mount'
    if (-not (Wait-Volume $Letter $true 30)) { throw "volume did not appear on ${Letter}:" }
    $vol = Get-Volume -DriveLetter $Letter
    Write-Step "mounted ${Letter}: $($vol.FileSystem) $([math]::Round($vol.Size / 1MB)) MB label=$($vol.FileSystemLabel)"
    if ($vol.FileSystem -ne $FileSystem) { throw "expected $FileSystem on ${Letter}:, got $($vol.FileSystem)" }
    Write-Result @{ action = 'mount'; outcome = 'ok'; letter = $Letter; fs = $vol.FileSystem; mb = [math]::Round($vol.Size / 1MB); seconds = $r.Seconds }
}

function Invoke-Attach([switch] $ReadOnly) {
    if (-not $Letter) { throw '-Letter is required for attach' }
    if ($Letter.ToUpperInvariant() -eq ([string] $env:SystemDrive).TrimEnd(':', '\').ToUpperInvariant()) {
        throw "refusing to put a test volume on the system drive letter ${Letter}:"
    }
    if ((Get-VhdImage).Attached) {
        # After 'detach -Method mountvol' the vdisk is still attached but its volume is
        # offline with no letter; giving it a mount point brings it back online.
        if ($ReadOnly) { throw "$script:FullPath is already attached" }
        Write-Step "$script:FullPath is still attached; only assigning ${Letter}:"
        Set-VolumeLetter $Letter
        Write-Result @{ action = 'attach'; outcome = 'ok'; letter = $Letter; method = 'mount-point-only' }
        return
    }
    $attach = if ($ReadOnly) { 'attach vdisk readonly' } else { 'attach vdisk' }
    $r = Invoke-Diskpart @("select vdisk file=`"$script:FullPath`"", $attach)
    Assert-Native $r "diskpart $attach"
    Set-VolumeLetter $Letter
    if (-not $ReadOnly) {
        Write-Result @{ action = 'attach'; outcome = 'ok'; letter = $Letter; method = 'diskpart'; seconds = $r.Seconds }
    }
}

function Invoke-Detach {
    if (-not $Letter) { throw '-Letter is required for detach' }
    if (-not (Test-VolumePresent $Letter)) { throw "no volume on ${Letter}: to detach" }
    Assert-LetterIsVhdx $Letter
    $requested = if ($Method) { $Method } else { 'auto' }
    if ($requested -eq 'auto') {
        $order = @('diskpart', 'mountvol', 'dismountdiskimage')
    } elseif ($requested -in 'diskpart', 'mountvol', 'dismountdiskimage') {
        $order = @($requested)
    } else {
        throw "-Method $requested is not a detach method"
    }
    $attempts = @()
    foreach ($m in $order) {
        Write-Step "detach ${Letter}: with $m"
        $r = switch ($m) {
            'diskpart' { Invoke-Diskpart @("select vdisk file=`"$script:FullPath`"", 'detach vdisk') }
            # /p removes the mount point and dismounts the volume; the vdisk stays attached.
            'mountvol' {
                Assert-LetterIsVhdx $Letter # re-checked right before /p takes the volume offline
                Invoke-Native 'mountvol.exe' "$($Letter.ToUpperInvariant()): /p" $TimeoutSec
            }
            'dismountdiskimage' { Invoke-DismountDiskImage }
        }
        $gone = Wait-Volume $Letter $false 15
        $attempts += "${m}:$($r.Status)/$(if ($gone) { 'gone' } else { 'present' })/$($r.Seconds)s"
        if ($gone) {
            Write-Step "volume ${Letter}: is gone after $m ($($r.Status), $($r.Seconds) s)"
            Write-Result @{ action = 'detach'; outcome = 'ok'; letter = $Letter; method = $m; seconds = $r.Seconds; attempts = ($attempts -join ',') }
            return
        }
        Write-Step "volume ${Letter}: still present after $m ($($r.Status))"
    }
    Write-Result @{ action = 'detach'; outcome = 'failed'; letter = $Letter; method = 'none'; attempts = ($attempts -join ',') }
    throw "no detach method made ${Letter}: vanish: $($attempts -join ', ')"
}

function Invoke-ReadOnly {
    if (-not $Letter) { throw '-Letter is required for readonly' }
    if (-not (Test-VolumePresent $Letter)) { throw "no volume on ${Letter}:" }
    Assert-LetterIsVhdx $Letter
    $m = if ($Method) { $Method } else { 'volume' }
    $L = $Letter.ToUpperInvariant()
    switch ($m) {
        'volume' {
            Assert-Native (Invoke-Diskpart @("select volume $L", 'attributes volume set readonly')) 'diskpart attributes volume'
        }
        'disk' {
            $n = (Get-VhdImage | Get-Disk).Number
            Assert-Native (Invoke-Diskpart @("select disk $n", 'attributes disk set readonly')) 'diskpart attributes disk'
        }
        'vdisk' {
            Assert-Native (Invoke-Diskpart @("select vdisk file=`"$script:FullPath`"", 'detach vdisk')) 'diskpart detach vdisk'
            if (-not (Wait-Volume $L $false 15)) { throw "volume ${L}: still present after detach" }
            Invoke-Attach -ReadOnly
        }
        default { throw "-Method $m is not a readonly method" }
    }
    if ($m -in 'volume', 'disk') {
        # Force a remount so the file system picks the attribute up; next access remounts.
        $r = Invoke-Native 'fsutil.exe' "volume dismount ${L}:" 30
        Assert-Native $r "fsutil volume dismount ${L}:"
        if (-not (Wait-Volume $L $true 30)) { throw "volume ${L}: did not come back after dismount" }
    }
    $vol = Get-Volume -DriveLetter $L
    # Does the file system report the volume as read-only? Preflight would rely on this flag.
    $info = Invoke-Native 'fsutil.exe' "fsinfo volumeinfo ${L}:" 30
    # fsutil lists FILE_READ_ONLY_VOLUME as a line mentioning read-only; the exact wording varies by build.
    $flag = if (-not $info.Ok) { 'unknown' } elseif ($info.Text -match '(?i)read[- ]?only') { 'yes' } else { 'no' }
    Write-Step "${L}: is now set read-only with $m (health=$($vol.HealthStatus), fsutil read-only flag=$flag)"
    Write-Result @{ action = 'readonly'; outcome = 'ok'; letter = $L; method = $m; fsutil_readonly = $flag }
}

function Invoke-Unmount {
    $image = Get-VhdImage
    if (-not $image.Attached) {
        Write-Step "$script:FullPath is not attached"
        Write-Result @{ action = 'unmount'; outcome = 'ok'; method = 'already-detached' }
        return
    }
    $r = Invoke-Diskpart @("select vdisk file=`"$script:FullPath`"", 'detach vdisk')
    $method = 'diskpart'
    if (-not $r.Ok) {
        $method = 'dismountdiskimage'
        Assert-Native (Invoke-DismountDiskImage) 'Dismount-DiskImage'
    }
    if ((Get-VhdImage).Attached) { throw "$script:FullPath is still attached" }
    Write-Result @{ action = 'unmount'; outcome = 'ok'; method = $method }
}

try {
    $principal = [Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'needs an elevated shell (diskpart, mountvol and fsutil require admin)'
    }
    $script:FullPath = [System.IO.Path]::GetFullPath($Path)
    if ($script:FullPath -notmatch '\.vhdx$') { throw "-Path must end in .vhdx, got $script:FullPath" }
    switch ($Action) {
        'mount' { Invoke-Mount }
        'attach' { Invoke-Attach }
        'detach' { Invoke-Detach }
        'readonly' { Invoke-ReadOnly }
        'unmount' { Invoke-Unmount }
    }
    exit 0
} catch {
    Write-Host "vhdx: $Action failed: $($_.Exception.Message)"
    Write-Host $_.ScriptStackTrace
    if ($ResultFile -and $Action -ne 'detach') {
        Write-Result @{ action = $Action; outcome = 'failed'; letter = $Letter; error = ($_.Exception.Message -replace '\s+', '_') }
    }
    exit 1
}
