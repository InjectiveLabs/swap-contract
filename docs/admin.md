# Admin Functionality

This file contains template transactions for performing the primary functions of managing the Swap contracts.

## Sentry Node

Link to [swagger](https://sentry.lcd.injective.network/swagger/).

### Examples

```bash
NODE=https://sentry.tm.injective.network:443
CHAIN_ID=injective-1
CONTRACT_ADDRESS=inj12yj3mtjarujkhcp6lg3klxjjfrx2v7v8yswgp9 # Helix Swap Contract
```

#### Subaccount Open Spot Orders

```bash
curl -X GET "https://sentry.lcd.injective.network/injective/exchange/v1beta1/spot/orders/$MARKET_ID/$SUBACCOUNT_ID" -H "accept: application/json" | jq .
```

## Wasm

### Store

```bash
injectived tx wasm store ./artifacts/swap-aarch64.wasm --from=sgt-account --gas=auto --gas-prices 500000000inj  --gas-adjustment 1.3 --yes --output=json --node=https://testnet.sentry.tm.injective.network:443 --chain-id='injective-888' | jq .
```

### Query

#### Config

```bash
injectived query wasm contract-state smart $CONTRACT_ADDRESS '{"get_config": {}}'  --node=https://sentry.tm.injective.network:443 --output=json | jq .
```
