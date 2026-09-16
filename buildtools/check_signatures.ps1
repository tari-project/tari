param(
    [string[]]$Files,
    [string]$ScanDir
)

Write-Host "=== Signature Check Script Starting ==="

# Show usage if no input provided
if (-not $Files -and -not $ScanDir) {
    Write-Warning @"
No input files or scan directory provided.

Usage examples:

  # Check specific files
  .\check_signatures.ps1 -Files "C:\path\to\file1.exe","C:\path\to\file2.exe"

  # Or scan a directory recursively for all .exe files
  .\check_signatures.ps1 -ScanDir "C:\path\to\build\folder"

Environment vars in CI:
  - TS_FILES: Space-separated list of EXE paths
  - MTS_SOURCE: Folder to scan for EXEs

"@
    exit 1
}

# Locate signtool.exe
$programFilesX86 = [System.Environment]::GetFolderPath("ProgramFilesX86")
$sdkBasePath = Join-Path $programFilesX86 "Windows Kits"

if (-Not (Test-Path $sdkBasePath)) {
    Write-Error "Windows Kits folder not found at $sdkBasePath!"
    exit 1
}

Write-Output "Searching for signtool.exe in: $sdkBasePath"

$signtoolPath = Get-ChildItem -Path $sdkBasePath -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match '\\x64\\' } |
                Select-Object -ExpandProperty FullName -First 1

if (-not $signtoolPath) {
    Write-Error "signtool.exe not found in Windows Kits folder!"
    exit 1
}

Write-Output "Found signtool.exe at: $signtoolPath"
Write-Host ""

# Resolve EXE files
$exeFiles = @()

if ($Files) {
    Write-Host "Using provided TS_FILES list"
    $exeFiles = $Files
} elseif ($ScanDir) {
    Write-Host "Scanning for .exe files in: $ScanDir"
    $exeFiles = Get-ChildItem -Path $ScanDir -Recurse -Filter *.exe | Select-Object -ExpandProperty FullName
} else {
    Write-Error "No input files or scan directory provided"
    exit 1
}

if (-not $exeFiles) {
    Write-Error "No .exe files found"
    exit 1
}

# Check each file
$failures = 0
foreach ($file in $exeFiles) {
    Write-Host "Checking: $file"

    if (!(Test-Path $file)) {
        Write-Warning "File does not exist: $file"
        $failures++
        continue
    }

    # Check with Get-AuthenticodeSignature
    $sig = Get-AuthenticodeSignature $file
    if ($sig.Status -ne 'Valid') {
        Write-Warning "Authenticode check failed: $($sig.Status)"
        $failures++
    } else {
        Write-Host "Authenticode signature valid: $($sig.SignerCertificate.Subject)"
    }

    # Check with signtool
    & "$signtoolPath" verify /pa "$file"
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "signtool verify failed for: $file"
        $failures++
    } else {
        Write-Host "signtool verify passed"
    }
}

if ($failures -gt 0) {
    Write-Error "Signature check failed for $failures file(s)"
    exit $failures
}

Write-Host "All files passed signature checks"
