# Dextra Protocol

A decentralized finance protocol built on Solana using the Anchor framework.

## Original Development Instructions

To run the validator:

```bash
solana-test-validator --ledger /tmp/solana-ledger --rpc-port 8898
```

If validator runs on a different port:

```bash
export ANCHOR_PROVIDER_URL="http://127.0.0.1:8898" && echo $ANCHOR_PROVIDER_URL && anchor test
```

Other related commands:

```bash
export ANCHOR_PROVIDER_URL="http://127.0.0.1:8898"; anchor test
```

## Docker Setup

This project includes a Docker setup for consistent development and testing across different environments.

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/)
- [Docker Compose](https://docs.docker.com/compose/install/)

### Getting Started with Docker

1. **Build the Docker image**:

   ```bash
   docker-compose build
   ```

2. **Start a shell session in the container**:

   ```bash
   docker-compose run dextra shell
   ```

3. **Run the test suite**:

   ```bash
   docker-compose run test
   ```

4. **Start a local Solana validator**:

   ```bash
   docker-compose up validator
   ```

5. **Build the Dextra program**:

   ```bash
   docker-compose run dextra build
   ```

6. **Deploy the Dextra program**:
   ```bash
   docker-compose run dextra deploy
   ```

### Docker Container Commands

When inside the Docker container, you can use the following commands:

- `help` - Show the help message
- `build` - Build the Dextra program
- `test` - Run the test suite
- `deploy [cluster]` - Deploy to a Solana cluster (localnet by default)
- `validator` - Start a local Solana validator
- `shell` - Start a shell session

### Volume Persistence

The Docker setup uses volumes to persist data:

- Your project files are mounted at `/app` in the container
- Solana config and validator data are persisted in the `dextra-data` volume
