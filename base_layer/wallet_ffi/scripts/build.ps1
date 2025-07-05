# Build script for Windows PowerShell

param(
    [string]$BuildMode = "release",
    [string]$Features = "python-bindings",
    [switch]$InstallDev
)

# Colors for output
$Red = "`e[31m"
$Green = "`e[32m"
$Yellow = "`e[33m"
$NC = "`e[0m"

function Write-Status {
    param([string]$Message)
    Write-Host "$Green[INFO]$NC $Message"
}

function Write-Warning {
    param([string]$Message)
    Write-Host "$Yellow[WARN]$NC $Message"
}

function Write-Error {
    param([string]$Message)
    Write-Host "$Red[ERROR]$NC $Message"
}

# Check prerequisites
Write-Status "Checking prerequisites..."

# Check Rust
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Error "Rust is not installed. Please install Rust 1.70+ from https://rustup.rs/"
    exit 1
}

# Check Python
if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Error "Python is not installed. Please install Python 3.8+ from https://python.org/"
    exit 1
}

# Check maturin
if (-not (Get-Command maturin -ErrorAction SilentlyContinue)) {
    Write-Warning "maturin is not installed. Installing..."
    pip install maturin
}

# Set environment variables
$env:TARI_TARGET_NETWORK = if ($env:TARI_TARGET_NETWORK) { $env:TARI_TARGET_NETWORK } else { "nextnet" }
$env:RUSTFLAGS = if ($env:RUSTFLAGS) { "$env:RUSTFLAGS -D warnings" } else { "-D warnings" }

Write-Status "Building with mode: $BuildMode, features: $Features"

# Clean previous builds if requested
if ($BuildMode -eq "clean") {
    Write-Status "Cleaning previous builds..."
    cargo clean
    Remove-Item -Path "target/wheels" -Recurse -Force -ErrorAction SilentlyContinue
    exit 0
}

# Build Rust library
Write-Status "Building Rust library..."
try {
    if ($BuildMode -eq "debug") {
        cargo build --features $Features
    } else {
        cargo build --release --features $Features
    }
} catch {
    Write-Error "Failed to build Rust library: $($_.Exception.Message)"
    exit 1
}

# Build Python wheel
Write-Status "Building Python wheel..."
try {
    if ($BuildMode -eq "debug") {
        maturin develop --features $Features
    } else {
        maturin develop --release --features $Features
    }
} catch {
    Write-Error "Failed to build Python wheel: $($_.Exception.Message)"
    exit 1
}

Write-Status "Build completed successfully!"

# Optional: Install development dependencies
if ($InstallDev) {
    Write-Status "Installing development dependencies..."
    pip install -r requirements-dev.txt
}

Write-Status "You can now import tari_wallet in Python!"
