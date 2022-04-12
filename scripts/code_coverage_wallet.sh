#!/bin/bash
set -e

REPORT_DIR="report/"
CRATE_DIR="../base_layer/wallet"
RELATIVE_TARGET_DIR="../../target/debug"

echo "Check if latest llvm-tools are installed:"
rustup component add llvm-tools-preview

echo "Check if grcov installed:"
if [ "$(command -v grcov)" ]
then
    echo "    + Already installed"
else
    echo "    + Installing.."
    cargo install grcov
fi

export LLVM_PROFILE_FILE="coverage_data-%p-%m.profraw"
export RUSTFLAGS="-Zinstrument-coverage"

echo "Running Wallet tests to produce coverage profiling data:"
cd $CRATE_DIR
# We force the tests to use a single thread. Something about the profiling breaks the Sqlite connection pool even though every test
# with a db uses its own unique pool connection to its own unique file  ¯\_(ツ)_/¯
cargo test -- --test-threads=1

echo "Clear Report directories:"
if [ -d "$report_dir" ]; then
    rm -rf $report_dir
    echo "    + Report directory removed"
else
    echo "    + Report directory already cleared"
fi


echo "Generating coverage report:"
grcov . -s . --binary-path $RELATIVE_TARGET_DIR -t html --branch --ignore-not-existing -o $REPORT_DIR

CUR_DIR=$(pwd)
echo "Coverage report can be found in $CUR_DIR/$REPORT_DIR"