# Dextra

to run the validator: solana-test-validator --ledger /tmp/solana-ledger --rpc-port 8898

if validor run in on wrong port: export ANCHOR_PROVIDER_URL="http://127.0.0.1:8898" && echo $ANCHOR_PROVIDER_URL && anchor test
other related command: export ANCHOR_PROVIDER_URL="http://127.0.0.1:8898"; anchor test
