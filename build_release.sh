#!/bin/bash
set -euo pipefail

ARCH=""

if [[ $(arch) = "arm64" ]]; then
  ARCH=-arm64
fi

docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer${ARCH}:0.17.0
