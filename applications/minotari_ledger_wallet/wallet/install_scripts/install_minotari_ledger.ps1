[CmdletBinding()]
param(
    [switch]$Help,
    [string]$Tag,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"
$MinPython = [version]"3.9"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Installer = Join-Path $ScriptDir "install_minotari_ledger.py"

if (-not (Test-Path $Installer)) {
    Write-Error "install_minotari_ledger.py was not found next to this launcher."
}

function Test-PythonCandidate {
    param(
        [string]$Command,
        [string[]]$PrefixArgs
    )

    $Resolved = Get-Command $Command -ErrorAction SilentlyContinue
    if (-not $Resolved) {
        return $null
    }

    $VersionArgs = @()
    if ($PrefixArgs) {
        $VersionArgs += $PrefixArgs
    }
    $VersionArgs += @("-c", "import sys; raise SystemExit(0 if sys.version_info >= (3, 9) else 1)")

    & $Resolved.Source @VersionArgs *> $null
    if ($LASTEXITCODE -eq 0) {
        return @{
            Path = $Resolved.Source
            PrefixArgs = @($PrefixArgs)
        }
    }

    return $null
}

function Find-Python {
    $Candidates = @(
        @{ Command = "python"; PrefixArgs = @() },
        @{ Command = "python3"; PrefixArgs = @() },
        @{ Command = "py"; PrefixArgs = @("-3") }
    )

    foreach ($Candidate in $Candidates) {
        $Python = Test-PythonCandidate -Command $Candidate["Command"] -PrefixArgs $Candidate["PrefixArgs"]
        if ($Python) {
            return $Python
        }
    }

    return $null
}

function Install-PythonIfApproved {
    $Winget = Get-Command winget -ErrorAction SilentlyContinue
    $CommandText = "winget install --id Python.Python.3.12 -e --source winget"

    if (-not $Winget) {
        Write-Error "Python $MinPython or newer is required. Install it from https://www.python.org/downloads/windows/ or install winget and run: $CommandText"
    }

    $Answer = Read-Host "Python $MinPython or newer is required. Run '$CommandText' now? [y/N]"
    if ($Answer -notmatch "^(y|yes)$") {
        Write-Error "Install Python $MinPython or newer and rerun this launcher."
    }

    & $Winget.Source install --id Python.Python.3.12 -e --source winget
    if ($LASTEXITCODE -ne 0) {
        Write-Error "winget failed to install Python. Install Python $MinPython or newer and rerun this launcher."
    }
}

$Python = Find-Python
if (-not $Python) {
    Install-PythonIfApproved
    $Python = Find-Python
}

if (-not $Python) {
    Write-Error "Python $MinPython or newer is still unavailable after installation."
}

$InstallerArgs = @()
if ($Help) {
    $InstallerArgs += "--help"
}
if ($Tag) {
    $InstallerArgs += @("--tag", $Tag)
}
if ($RemainingArgs) {
    $InstallerArgs += $RemainingArgs
}

$PythonArgs = @()
if ($Python["PrefixArgs"]) {
    $PythonArgs += $Python["PrefixArgs"]
}
$PythonArgs += @($Installer)
$PythonArgs += $InstallerArgs

& $Python["Path"] @PythonArgs
exit $LASTEXITCODE
