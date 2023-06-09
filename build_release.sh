#!/bin/bash
ARCH=""

if [[ $(arch) = "arm64" ]]; then
  ARCH=-arm64
fi

docker run --rm -v "$(pwd)":/code -v "$HOME/.cargo/git":/usr/local/cargo/git \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer${ARCH}:0.12.13
