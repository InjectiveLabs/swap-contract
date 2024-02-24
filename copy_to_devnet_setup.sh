#!/bin/bash
ARCH=""

if [[ $(arch) = "arm64" ]]; then
  ARCH=-aarch64
fi

COMMIT_HASH=$(git rev-parse --short HEAD)

rm -f ../devnet-setup/wasm-contracts/swap_contract*
cp artifacts/swap_contract$ARCH.wasm ../devnet-setup/wasm-contracts/swap_contract_${COMMIT_HASH}.wasm

echo "SWAP_CONVERTER_COMMIT_HASH=$COMMIT_HASH" > "../devnet-setup/wasm-contracts/swap_contract.version"