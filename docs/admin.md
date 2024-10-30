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

#### Subaccount Open Derivative Orders

```bash
curl -X GET "https://sentry.lcd.injective.network/injective/exchange/v1beta1/derivative/orders/$MARKET_ID/$SUBACCOUNT_ID" -H "accept: application/json" | jq .
```

#### Subaccount Deposits

```bash
curl -X GET "https://sentry.lcd.injective.network/injective/exchange/v1beta1/exchange/subaccountDeposits?subaccount_id=$SUBACCOUNT_ID" -H "accept: application/json" | jq .
```

## Wasm

### Store

```bash
injectived tx wasm store ./artifacts/grid-aarch64.wasm --from=sgt-account --gas=auto --gas-prices 500000000inj  --gas-adjustment 1.3 --yes --output=json --node=https://testnet.sentry.tm.injective.network:443 --chain-id='injective-888' | jq .
```

### Instantiate - Derivative

```bash
injectived tx wasm instantiate $CODE_ID '{"market_type": "derivative", "base_decimals": 18, "quote_decimals": 6, "market_id": "0x17ef48032cb24375ba7c2e39f384e56433bcab20cbee9a7357e4cba2eb00abe6", "small_order_threshold": "10.0"}' --label="inj-usdt-pgt" --admin sgt-account --from=sgt-account --gas=auto --gas-prices 500000000inj --gas-adjustment 1.3 --output=json --node=https://testnet.sentry.tm.injective.network:443 --chain-id='injective-888'
```

### Instantiate - Spot

```bash
injectived tx wasm instantiate $CODE_ID '{"market_type": "spot", "base_decimals": 6, "quote_decimals": 6, "market_id": "0x42edf70cc37e155e9b9f178e04e18999bc8c404bd7b638cc4cbf41da8ef45a21", "valuation_market_id": "0xa508cb32923323679f29a032c70342c147c17d0145625922b0ef22e955c844c0", "small_order_threshold": "10.0"}' --label="qunt-inj-sgt" --admin sgt-account --from=sgt-account --gas=auto --gas-prices 500000000inj --gas-adjustment 1.3 --output=json --node=https://sentry.tm.injective.network:443 --chain-id='injective-1'
```

injectived tx wasm instantiate 685 '{"market_type": "spot", "base_decimals": 8, "quote_decimals": 6, "market_id": "0xb03ead807922111939d1b62121ae2956cf6f0a6b03dfdea8d9589c05b98f670f", "small_order_threshold": "10.0"}' --label="w-usdt-sgt" --admin sgt-account --from=sgt-account --gas=auto --gas-prices 500000000inj --gas-adjustment 1.3 --output=json --node=https://sentry.tm.injective.network:443 --chain-id='injective-1'

### Execute

#### Create Strategy - Trailing

```bash
injectived tx wasm execute $CONTRACT_ADDRESS "{'create_strategy': {'subaccount_id': '$SUBACCOUNT_ID', 'bounds': ['1.0', 1.1], 'levels': 10, 'strategy_type': {'trailing_arithmetc': {'lower_trailing_bound': '0.5', 'upper_trailing_bound': '1.5'}}}}"  --from=sgt-account --gas=auto --gas-prices 500000000inj --gas-adjustment 1.3 --output=json --yes --node=https://sentry.tm.injective.network:443 --chain-id='injective-1' | jq .
```

### Query

#### Config

```bash
injectived query wasm contract-state smart $CONTRACT_ADDRESS '{"get_config": {}}'  --node=https://sentry.tm.injective.network:443 --output=json | jq .
```
