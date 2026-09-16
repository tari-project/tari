#!/usr/bin/env bash
## DEPRECATION NOTICE ##
#
# This script is deprecated and will be removed in a future release.
# Use the source_coverage.sh script for code coverage tests instead.
#

# Get package to test
package_arg="--workspace --exclude tari_integration_tests"

# If first argument is not empty, set it to package variable
if [ -n "$1" ]; then
    package="$1"
    package_arg="-p $package"
fi

source_root_dir="tari"
build_dir="target/debug/"
report_dir="report/$package"

echo "Check if in correct directory":
path=$(pwd)
primary_dir=$(basename $path)
if [ "$primary_dir" == "scripts" ]; then
    echo "    + Moving to correct directory.."
    cd ..
fi

# Check if in Tari primary folder before proceeding
path=$(pwd)
primary_dir=$(basename $path)
if [ "$primary_dir" == "$source_root_dir" ]; then
    echo "    + Correct directory"
else
    echo "    + Error: Incorrect directory -> start code_coverage from script or tari folder!"
    exit 1
fi

echo "Check for llvm-preview tools"
if [ "$(cargo cov --version)" ]
then
    echo "    + Already installed"
else
    echo "    + Installing.."
    rustup component add llvm-tools-preview
fi


echo "Check if grcov v0.8 installed:"
if [[ "$(grcov --version)" == "grcov 0.8"* ]]
then
    echo "    + grcov v0.8 is already installed"
else
    echo "    + Installing.."
    cargo install grcov
fi

if [ -d "$report_dir" ]; then
    rm -rf $report_dir
    echo "    + Report directory removed"
else
    echo "    + Report directory already cleared"
fi
mkdir -p $report_dir

echo "Setup project.."
export RUSTFLAGS="-C instrument-coverage"
export RUSTDOCFLAGS="-C instrument-coverage"
export LLVM_PROFILE_FILE="coverage_data-%p-%m.profraw"
export CARGO_UNSTABLE_SPARSE_REGISTRY=true

echo "Building $package..."
cargo test --all-features --no-fail-fast ${package_arg}

grcov . -s . --binary-path ${build_dir} -t html --branch --ignore-not-existing \
             -o ${report_dir} \
             --ignore target/**/*.rs \
             --ignore **/.cargo/**/*.rs

echo "Cleaning up temporary files.."
rm coverage_data-*.profraw

echo "Launch report in browser.."
index_str="index.html"
open "$report_dir/html/$index_str"
