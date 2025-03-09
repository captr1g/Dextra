#!/bin/bash
set -e

# Function to display help message
show_help() {
  echo "Dextra Project Docker Container"
  echo "--------------------------------"
  echo "Available commands:"
  echo "  help        - Show this help message"
  echo "  build       - Build the Dextra program"
  echo "  test        - Run the test suite"
  echo "  deploy      - Deploy to a Solana cluster (localnet by default)"
  echo "  shell       - Start a shell session"
  echo "  validator   - Start a local Solana validator"
  echo ""
  echo "Example: docker run -it dextra test"
}

# Start a local validator if not already running
ensure_validator_running() {
  if ! solana config get | grep -q "RPC URL: http://127.0.0.1:8899"; then
    solana config set --url localhost
  fi
  
  # Check if validator is already running
  if ! solana cluster-version &>/dev/null; then
    echo "Starting local Solana validator..."
    solana-test-validator --quiet --reset &
    sleep 5
  fi
}

case "$1" in
  help)
    show_help
    ;;
    
  build)
    echo "Building Dextra program..."
    anchor build
    ;;
    
  test)
    echo "Running Dextra tests..."
    ensure_validator_running
    anchor test --skip-local-validator
    ;;
    
  deploy)
    echo "Deploying Dextra program..."
    CLUSTER=${2:-localnet}
    ensure_validator_running
    anchor deploy --provider.cluster $CLUSTER
    ;;
    
  validator)
    echo "Starting Solana validator..."
    solana-test-validator --reset
    ;;
    
  shell)
    echo "Starting shell..."
    /bin/bash
    ;;
    
  *)
    show_help
    ;;
esac 