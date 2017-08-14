#!/bin/bash

set -e

function usage() {
    echo "usage: $0 <spec path> [rust | test]"
    exit -1
}

if [ $# -eq 1 ]; then
    SPEC=$1
    RUST=yes
    TEST=yes
elif [ $# -eq 2 ]; then
    SPEC=$1
    if [ $2 == "rust" ]; then
        RUST=yes
        TEST=
    elif [ $2 == "test" ]; then
        RUST=
        TEST=yes
    else
        usage
    fi
else
    usage
fi

ALL=$(find -L $SPEC -name '*.stone')

if [[ $RUST ]]; then
    # Generate the Rust code
    rm -rf src/generated
    mkdir -p src/generated
    PYTHONPATH=stone python2.7 -m stone.cli -v generator/rust.stoneg.py --attribute :all src/generated $ALL
fi

if [[ $TEST ]]; then
    # Generate test code
    rm -rf tests/generated
    mkdir -p tests/generated
    mkdir -p tests/generated/reference
    PYTHONPATH=stone python2.7 -m stone.cli -v generator/test.stoneg.py tests/generated $ALL
fi
