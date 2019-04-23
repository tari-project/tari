#!/bin/bash

# Check if in script directory
path=$(pwd)
primary_dir=$(basename $path)
if [ "$primary_dir" != "scripts" ]; then
    cd scripts
fi

./code_coverage.sh "storage" "infrastructure/storage/"
