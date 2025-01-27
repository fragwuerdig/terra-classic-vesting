#! /bin/bash

set -e

rm -Rf artifacts
mkdir artifacts
cargo build --release --target wasm32-unknown-unknown

mv target/wasm32-unknown-unknown/release/*.wasm artifacts/
rm -Rf target

cd artifacts
mkdir -p opt
for i in *.wasm; do
    wasm-opt -Os --strip-debug $i -o opt/$i
done