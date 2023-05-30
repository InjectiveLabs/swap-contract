#!/bin/bash
ARCH=""

if [[ $(arch) = "arm64" ]]; then
  ARCH=-aarch64
fi

COMMIT_HASH=$(git rev-parse --short HEAD)

rm -f ../devnet-setup/wasm-contracts/swap_converter*.*
cp artifacts/helix_converter$ARCH.wasm ../devnet-setup/wasm-contracts/swap_converter_${COMMIT_HASH}.wasm

echo "COMMIT_HASH=$COMMIT_HASH" > "../devnet-setup/wasm-contracts/swap_converter.version"