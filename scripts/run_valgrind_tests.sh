#!/bin/bash
#
# Valgrind Memory Leak Detection Script
# 
# Cross-platform memory testing for Tari wallet FFI callback system.
# Runs existing memory safety tests under valgrind on Linux or uses
# macOS leaks tool for memory leak detection.

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WALLET_FFI_DIR="$PROJECT_ROOT/base_layer/wallet_ffi"
SUPPRESSIONS_FILE="$SCRIPT_DIR/valgrind_suppressions.txt"
LOG_DIR="$PROJECT_ROOT/target/valgrind_logs"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create log directory
mkdir -p "$LOG_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Platform detection
detect_platform() {
    case "$(uname -s)" in
        Linux*)
            echo "linux"
            ;;
        Darwin*)
            echo "macos"
            ;;
        CYGWIN*|MINGW32*|MSYS*|MINGW*)
            echo "windows"
            ;;
        *)
            echo "unknown"
            ;;
    esac
}

# Check if valgrind is available (Linux only)
check_valgrind() {
    if ! command -v valgrind &> /dev/null; then
        log_error "Valgrind not found. Please install valgrind:"
        log_info "Ubuntu/Debian: sudo apt-get install valgrind"
        log_info "CentOS/RHEL: sudo yum install valgrind"
        log_info "Fedora: sudo dnf install valgrind"
        return 1
    fi
    
    local valgrind_version
    valgrind_version=$(valgrind --version | head -n1)
    log_info "Found $valgrind_version"
    return 0
}

# Check if leaks tool is available (macOS only)
check_leaks_tool() {
    if ! command -v leaks &> /dev/null; then
        log_error "leaks tool not found. Please install Xcode command line tools:"
        log_info "xcode-select --install"
        return 1
    fi
    
    log_info "Found macOS leaks tool"
    return 0
}

# Build test binary with debug symbols
build_tests() {
    log_info "Building test binary with debug symbols..."
    
    cd "$WALLET_FFI_DIR"
    
    # Use debug profile with additional memory debugging flags
    RUSTFLAGS="-C debuginfo=2 -C opt-level=0 -Z sanitizer=address" \
    cargo build --profile=dev --tests --features memory-testing 2>&1 | tee "$LOG_DIR/build_${TIMESTAMP}.log"
    
    if [ ${PIPESTATUS[0]} -ne 0 ]; then
        log_error "Failed to build tests"
        return 1
    fi
    
    log_success "Tests built successfully"
    return 0
}

# Run valgrind on Linux
run_valgrind_linux() {
    log_info "Running valgrind memory leak detection..."
    
    local test_binary
    test_binary=$(find "$WALLET_FFI_DIR/target/debug/deps" -name "*callback_leaks*" -type f -executable | head -n1)
    
    if [ -z "$test_binary" ]; then
        log_error "Memory leak test binary not found"
        return 1
    fi
    
    log_info "Found test binary: $test_binary"
    
    # Valgrind options optimized for memory leak detection
    local valgrind_opts=(
        --tool=memcheck
        --leak-check=full
        --show-leak-kinds=all
        --track-origins=yes
        --verbose
        --error-exitcode=1
        --suppressions="$SUPPRESSIONS_FILE"
        --gen-suppressions=all
        --log-file="$LOG_DIR/valgrind_${TIMESTAMP}.log"
    )
    
    log_info "Valgrind options: ${valgrind_opts[*]}"
    
    # Run the memory leak tests under valgrind
    if valgrind "${valgrind_opts[@]}" "$test_binary" --nocapture 2>&1 | tee "$LOG_DIR/valgrind_output_${TIMESTAMP}.log"; then
        log_success "Valgrind completed successfully - no memory leaks detected"
        return 0
    else
        log_error "Valgrind detected memory issues - check logs in $LOG_DIR"
        return 1
    fi
}

# Run leaks tool on macOS
run_leaks_macos() {
    log_info "Running macOS leaks detection..."
    
    local test_binary
    test_binary=$(find "$WALLET_FFI_DIR/target/debug/deps" -name "*callback_leaks*" -type f -executable | head -n1)
    
    if [ -z "$test_binary" ]; then
        log_error "Memory leak test binary not found"
        return 1
    fi
    
    log_info "Found test binary: $test_binary"
    
    # Run the test and capture its PID for leak detection
    log_info "Starting test process for leak detection..."
    
    # Run test in background to get PID
    "$test_binary" --nocapture &
    local test_pid=$!
    
    # Wait a moment for test to start
    sleep 1
    
    # Check if process is still running
    if ! kill -0 "$test_pid" 2>/dev/null; then
        log_warning "Test process finished too quickly for leak detection"
        wait "$test_pid"
        local exit_code=$?
        if [ $exit_code -eq 0 ]; then
            log_success "Tests passed, no runtime errors detected"
        else
            log_error "Tests failed with exit code $exit_code"
            return $exit_code
        fi
    else
        # Run leaks detection
        if leaks "$test_pid" > "$LOG_DIR/leaks_${TIMESTAMP}.log" 2>&1; then
            log_success "No memory leaks detected by macOS leaks tool"
            # Wait for test to complete
            wait "$test_pid"
            return $?
        else
            log_error "Memory leaks detected - check $LOG_DIR/leaks_${TIMESTAMP}.log"
            # Kill the test process
            kill "$test_pid" 2>/dev/null || true
            return 1
        fi
    fi
    
    return 0
}

# Run memory tests on Windows (simplified)
run_windows_tests() {
    log_warning "Windows memory leak detection limited to runtime tests"
    log_info "Running memory safety tests without valgrind..."
    
    cd "$WALLET_FFI_DIR"
    
    if cargo test --test test_callback_leaks --features memory-testing -- --nocapture 2>&1 | tee "$LOG_DIR/windows_memory_${TIMESTAMP}.log"; then
        log_success "Memory safety tests passed on Windows"
        return 0
    else
        log_error "Memory safety tests failed"
        return 1
    fi
}

# Parse valgrind/leaks output for actionable errors
parse_results() {
    local platform="$1"
    
    case "$platform" in
        linux)
            if [ -f "$LOG_DIR/valgrind_${TIMESTAMP}.log" ]; then
                log_info "Parsing valgrind results..."
                
                # Check for definite leaks
                local definite_leaks
                definite_leaks=$(grep -c "definitely lost" "$LOG_DIR/valgrind_${TIMESTAMP}.log" || echo "0")
                
                # Check for possible leaks  
                local possible_leaks
                possible_leaks=$(grep -c "possibly lost" "$LOG_DIR/valgrind_${TIMESTAMP}.log" || echo "0")
                
                # Check for errors
                local errors
                errors=$(grep -c "ERROR SUMMARY" "$LOG_DIR/valgrind_${TIMESTAMP}.log" || echo "0")
                
                log_info "Definite leaks: $definite_leaks"
                log_info "Possible leaks: $possible_leaks"
                log_info "Errors: $errors"
                
                if [ "$definite_leaks" -gt 0 ] || [ "$possible_leaks" -gt 0 ]; then
                    log_error "Memory leaks detected in valgrind output"
                    return 1
                fi
            fi
            ;;
        macos)
            if [ -f "$LOG_DIR/leaks_${TIMESTAMP}.log" ]; then
                log_info "Parsing macOS leaks results..."
                
                if grep -q "0 leaks for 0 total leaked bytes" "$LOG_DIR/leaks_${TIMESTAMP}.log"; then
                    log_success "No leaks found by macOS leaks tool"
                else
                    log_error "Potential leaks detected - check logs"
                    return 1
                fi
            fi
            ;;
    esac
    
    return 0
}

# Create summary report
create_summary() {
    local platform="$1"
    local status="$2"
    
    local summary_file="$LOG_DIR/memory_test_summary_${TIMESTAMP}.txt"
    
    cat > "$summary_file" << EOF
Tari Wallet FFI Memory Leak Detection Summary
============================================

Timestamp: $(date)
Platform: $platform
Status: $status
Test Binary: callback_leaks
Logs Directory: $LOG_DIR

Test Results:
EOF

    case "$platform" in
        linux)
            echo "- Valgrind Tool: memcheck with full leak detection" >> "$summary_file"
            if [ -f "$LOG_DIR/valgrind_${TIMESTAMP}.log" ]; then
                echo "- Valgrind Log: valgrind_${TIMESTAMP}.log" >> "$summary_file"
                grep "ERROR SUMMARY" "$LOG_DIR/valgrind_${TIMESTAMP}.log" >> "$summary_file" || true
            fi
            ;;
        macos)
            echo "- macOS leaks tool used for detection" >> "$summary_file"
            if [ -f "$LOG_DIR/leaks_${TIMESTAMP}.log" ]; then
                echo "- Leaks Log: leaks_${TIMESTAMP}.log" >> "$summary_file"
                tail -n 5 "$LOG_DIR/leaks_${TIMESTAMP}.log" >> "$summary_file" || true
            fi
            ;;
        windows)
            echo "- Windows runtime memory safety tests" >> "$summary_file"
            ;;
    esac
    
    log_info "Summary report created: $summary_file"
}

# Main execution
main() {
    log_info "Starting Tari Wallet FFI Memory Leak Detection"
    log_info "Project root: $PROJECT_ROOT"
    
    local platform
    platform=$(detect_platform)
    log_info "Detected platform: $platform"
    
    # Platform-specific setup and execution
    case "$platform" in
        linux)
            if ! check_valgrind; then
                exit 1
            fi
            
            if ! build_tests; then
                exit 1
            fi
            
            if run_valgrind_linux && parse_results "$platform"; then
                create_summary "$platform" "PASSED"
                log_success "Memory leak detection completed successfully"
                exit 0
            else
                create_summary "$platform" "FAILED"
                log_error "Memory leak detection failed"
                exit 1
            fi
            ;;
        macos)
            if ! check_leaks_tool; then
                exit 1
            fi
            
            if ! build_tests; then
                exit 1
            fi
            
            if run_leaks_macos && parse_results "$platform"; then
                create_summary "$platform" "PASSED"
                log_success "Memory leak detection completed successfully"
                exit 0
            else
                create_summary "$platform" "FAILED"
                log_error "Memory leak detection failed"
                exit 1
            fi
            ;;
        windows)
            if run_windows_tests; then
                create_summary "$platform" "PASSED"
                log_success "Memory safety tests completed successfully"
                exit 0
            else
                create_summary "$platform" "FAILED"
                log_error "Memory safety tests failed"
                exit 1
            fi
            ;;
        *)
            log_error "Unsupported platform: $platform"
            log_info "Supported platforms: Linux (valgrind), macOS (leaks), Windows (runtime tests)"
            exit 1
            ;;
    esac
}

# Handle script arguments
if [ $# -gt 0 ] && [ "$1" = "--help" ]; then
    cat << EOF
Tari Wallet FFI Memory Leak Detection Script

Usage: $0 [--help]

This script runs memory leak detection tests for the Tari wallet FFI system:
- Linux: Uses valgrind with full leak checking
- macOS: Uses system leaks tool
- Windows: Runs runtime memory safety tests

Logs are saved to: $LOG_DIR
Suppression file: $SUPPRESSIONS_FILE

Requirements:
- Linux: valgrind package installed
- macOS: Xcode command line tools
- Windows: None (uses built-in cargo test)

The script builds debug versions of memory tests and runs them under
appropriate memory detection tools for the current platform.
EOF
    exit 0
fi

# Run main function
main "$@"
