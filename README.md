# Atomic Order Token Swap Contract

The Swap contract allows instantly swapping between different tokens, using atomic market orders. If there’s no market that would allow performing such swap directly, contract will perform a necessary number of intermediary trades (for example if user wants to swap INJ to ATOM, contract will sell INJ to USDT and then buy ATOM with USDT).

## Messages

### Instantiate

Initializes the contract state with the contract version and configuration details. The config includes an administrator address and a fee recipient address.

```rust
pub fn instantiate(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError>
```

### Execute

Handles different types of transactions and admin functions:

- SwapMinOutput: Swap with the minimum output quantity.
- SwapExactOutput: Swap with an exact output quantity.
- SetRoute: Set a swap route.
- DeleteRoute: Delete a swap route.
- UpdateConfig: Update the contract configuration.
- WithdrawSupportFunds: Withdraw the support funds from the contract.

```rust
pub fn execute(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError>
```

### Reply

Handles the replies from other contracts or transactions.

```rust
pub fn reply(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    msg: Reply,
) -> Result<Response<InjectiveMsgWrapper>, ContractError>
```

### Query

Handles various queries to the contract:

- GetRoute: Get a specific swap route.
- GetOutputQuantity: Get the output quantity for a given input quantity.
- GetInputQuantity: Get the input quantity for a given output quantity.
- GetAllRoutes: Get all available swap routes.

```rust
pub fn query(deps: Deps<InjectiveQueryWrapper>, env: Env, msg: QueryMsg) -> StdResult<Binary>
```

## Leveraging the Atomic Order Token Swap Contract for Your Injective dApp

The Atomic Order Token Swap Contract offers a seamless integration for token swapping capabilities within your dApp or as an external smart contract.

There are 2 main ways you can use it:

### 1 - Utilization of HelixApp's Swap Contract

The contract is readily available on [Mainnet](https://explorer.injective.network/contract/inj1psk3468yr9teahgz73amwvpfjehnhczvkrhhqx/).

Advantages:

- Quick up and running token swapping capability.
- No need to lock up funds into the contract.

Limitations:

- Absence of configurability: You cannot set up or delete routes, update configurations, or withdraw support funds.
- Fee Allocation: Any generated fees will be directed towards HelixApp.

### 2 - New instance of our stored code

The code is readily available on [Mainnet](https://explorer.injective.network/code/67/), and you can instanciate it too.

Advantages:

- You retain complete control over the available routes.
- Administrative Privileges: Full administrative access to the contract.
- Fee Collection: Ability to collect fees.

Limitations:

- Need to lock around 500 USDT in the contract to guarantee its functionality.

Instantiate command:

```bash
INIT='{"admin":"'$YOUR_ADMIN_ADDRESS'", "fee_recipient":{"address": "'$YOUR_FEE_RECIPIENT_ADDRESS'"}}'
INSTANTIATE_TX_HASH=$(yes 12345678 | injectived tx wasm instantiate $code_id "$INIT" \
--label="Your dApp Swap Contract" \
--from=$USER --chain-id="$CHAIN_ID" --yes --admin=$USER $HOME --node=$NODE \
$GAS_AND_FEE $SYNC_MODE $KBT
```

### 3 - Upload and Instantiate Your Contract

Advantages:

- You retain complete control over the available routes.
- Administrative Privileges: Full administrative access to the contract.
- Fee Collection: Ability to collect fees.

Limitations:

- Complexity: This method requires a better understanding and needs a governance proposal.
- Need to lock around 500 USDT in the contract to guarantee its functionality.



### FE Integration

@Shane here it goes info on how we integrated it for Helix

### Disclaimer

This contract is developed with precision and rigor but hasn't undergone an external audit. Proceed with discretion, understanding that use of this contract comes with inherent risks.

## How to Use

Install [cargo-make](https://sagiegurari.github.io/cargo-make/):

```sh
cargo install --force cargo-make
```

Run formatter:

```sh
cargo make fmt
```

Run tests:

```sh
cargo make test
```

Run linter (clippy):

```sh
cargo make lint
```

Check for unused dependencies:

```sh
cargo make udeps
```

Compile all contracts using [rust-optimizer](https://github.com/CosmWasm/rust-optimizer):

```sh
cargo make optimize
```

Once optimized, verify the wasm binaries are ready to be uploaded to the blockchain:

```sh
cargo make check
```

Generate JSON schema for all contracts:

```sh
cargo make schema
```

Publish contracts and packages to [crates.io](https://crates.io/):

```sh
cargo make publish
```

**NOTE:** For the last two tasks (schema and publish), you need to update the shell script in [`Makefile.toml`](./Makefile.toml) for them to work.

## License

Contents of this repository are open source under [GNU Affero General Public License](./LICENSE) v3 or later.
