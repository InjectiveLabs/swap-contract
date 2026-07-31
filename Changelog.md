# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [1.1.3] - 2026-07-31

### Added

- Added migration paths for the mainnet Code 67 and Code 1962 lineages.

### Changed

- Updated the Injective test stack to mainnet 1.20.0 and Rust to 1.86.0.
- Updated release builds to `cosmwasm/optimizer:0.17.0`, matching the RFQ release flow.
- Updated `FPDecimal` coin conversion for CosmWasm 3 while preserving integer floor behavior.

## [1.1.0] - 2024-10-30

### Fixed

- Added pagination to `get_all_routes`
- Bump version of `serde-json-wasm`

## [1.0.1] - 2024-02-24

### Added

- This Changelog file
- `rust-toolchain` file
- Contract migration templates and migrate function

### Fixed

- Correctly round to `min_quantity_tick_size` in intermediate steps for multi-hop buys

## [Version v1.0.0] - 2024-02-16

### Added

- Versioning into contract

### Changed

- Updated to latest CosmWasm version
