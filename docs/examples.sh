# Variables
KBT="--keyring-backend test"
CHAIN_ID="injective-1"
HOME="--home ."
GAS_AND_FEE="--gas=6000000 --gas-prices=500000000inj"
NODE="https://k8s.global.mainnet.tm.injective.network:443"
SYNC_MODE="--broadcast-mode sync"
USER="swap-exec"
code_id="67"
ADMIN="inj1exmuhajlxg08l4a59rchsjycxk42dgydg7u62l"

# MAINNET MARKETS
INJUSDT=0xa508cb32923323679f29a032c70342c147c17d0145625922b0ef22e955c844c0
ATOMUSDT=0x0511ddc4e6586f3bfe1acb2dd905f8b8a82c97e1edaef654b12ca7e6031ca0fa
WETHUSDT=0xd1956e20d74eeb1febe31cd37060781ff1cb266f49e0512b446a5fafa9a16034
WMATICUSDT=0xb9a07515a5c239fcbfa3e25eaa829a03d46c4b52b9ab8ee6be471e9eb0e9ea31
USDCUSDCET=0xda0bb7a7d8361d17a9d2327ed161748f33ecbf02738b45a7dd1d812735d1531c
SOMMUSDT=0x0686357b934c761784d58a2b8b12618dfe557de108a220e06f8f6580abb83aab
GFUSDT=0x7f71c4fba375c964be8db7fc7a5275d974f8c6cdc4d758f2ac4997f106bb052b

# MAINNET DENOMS
USDT=peggy0xdAC17F958D2ee523a2206206994597C13D831ec7
WETH=peggy0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2
ATOM=ibc/C4CFF46FD6DE35CA4CF4CE031E643C8FDC9BA4B99AE598E9B0ED98FE3A2319F9
WMATIC=factory/inj14ejqjyq8um4p3xfqj74yld5waqljf88f9eneuk/inj1dxv423h8ygzgxmxnvrf33ws3k94aedfdevxd8h
SOL=factory/inj14ejqjyq8um4p3xfqj74yld5waqljf88f9eneuk/inj1sthrn5ep8ls5vzz8f9gp89khhmedahhdkqa8z3
SOMM=ibc/34346A60A95EB030D62D6F5BDD4B745BE18E8A693372A8A347D5D53DBBB1328B
USDCET=factory/inj14ejqjyq8um4p3xfqj74yld5waqljf88f9eneuk/inj1q6zlut7gtkzknkk773jecujwsdkgq882akqksk
GF=peggy0xAaEf88cEa01475125522e117BFe45cF32044E238



# INSTANTIATE 
INIT='{"admin":"'$ADMIN_ADDRESS'", "fee_recipient":{"address": "'$FEE_RECIPIENT_ADDRESS'"}}'
INSTANTIATE_TX_HASH=$(yes 12345678 | injectived tx wasm instantiate $CODE_ID "$INIT" --label="Your Swap Contract" \
--from=$USER --chain-id="$CHAIN_ID" --yes --admin=$USER $HOME --node=$NODE \
$GAS_AND_FEE $SYNC_MODE $KBT )

# SET ROUTE
INJ_USDT_ROUTE='{"set_route":{"source_denom":"inj","target_denom":"'$USDT'","route":["'$INJUSDT'"]}}'
TX_HASH=$(injectived tx wasm execute $SWAP_CONTRACT_ADDRESS "$INJ_USDT_ROUTE" $HOME --from=$USER $KBT --chain-id=$CHAIN_ID --yes $GAS_AND_FEE --node=$NODE | grep txhash | awk '{print $2}')