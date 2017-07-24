#!/bin/bash

if [ $# -ne 1 ]; then
    echo "usage: $0 <spec path>"
    exit -1
fi
SPEC=$1

ALL=$(find -L $SPEC -name '*.stone')

rm -rf src/generated
mkdir -p src/generated
PYTHONPATH=stone python -m stone.cli -v generator/rust.stoneg.py src/generated $ALL
