#!/usr/bin/env bash
set -e

# Get member source directory and name from command line arguments
if [ "$1" == "" -o "$2" == "" ]; then
    echo "Command line argument member_crate_name or member_source_dir missing, Usage: ./codecoverage.sh member_crate_name member_source_dir"
    exit 1
fi
member_crate_name=$1
member_source_dir=$2
source_root_dir="tari"
build_dir="target/debug/"
report_dir="report/$member_crate_name"

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

echo "Check if grcov installed:"
if [ "$(command -v grcov)" ]
then
    echo "    + Already installed"
else
    echo "    + Installing.."
    cargo install grcov
fi

echo "Check if lcov installed:"
if [ "$(command -v lcov)" ]
then
    echo "    + Already installed"
else
    echo "    + Installing.."
    ruby -e "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install)" < /dev/null 2> /dev/null
    brew install lcov
fi

echo "Clear Build and Report directories:"
if [ -d "$build_dir" ]; then
    rm -rf $build_dir
    echo "    + Build directory removed"
else
    echo "    + Build directory already cleared"
fi
if [ -d "$report_dir" ]; then
    rm -rf $report_dir
    echo "    + Report directory removed"
else
    echo "    + Report directory already cleared"
fi
# Make clean directories for Build and Report
mkdir $build_dir
mkdir -p $report_dir

echo "Build project.."
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Cinline-threshold=0 -Clink-dead-code -Coverflow-checks=off -Zno-landing-pads"
cargo +nightly build -q $CARGO_OPTIONS

echo "Perform project Tests.."
cargo_filename="Cargo.toml"
cargo +nightly test $CARGO_OPTIONS --manifest-path="$member_source_dir$cargo_filename"

echo "Acquire all build and test files for coverage check.."
ccov_filename="ccov.zip"
ccov_path="$report_dir$ccov_filename"

zip $ccov_path `find $build_dir \( -name "$member_crate_name*.gc*" \) -print`;

echo "Perform grcov code coverage.."
lcov_filename="lcov.info"
lcov_path="$report_dir$lcov_filename"
grcov $ccov_path -s . -t lcov --llvm --branch --ignore-not-existing --ignore-dir "/*" > $lcov_path;

echo "Generate report from code coverage.."
local_lcov_path="$report_dir$lcov_filename"
genhtml -o $report_dir --show-details --highlight --title $member_crate_name --legend $local_lcov_path

echo "Launch report in browser.."
index_str="index.html"
open "$report_dir/$index_str"
