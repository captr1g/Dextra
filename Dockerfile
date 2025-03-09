FROM rust:1.84.0-slim-bullseye

# Set environment variables
ENV PATH="/root/.cargo/bin:/root/.local/share/solana/install/active_release/bin:${PATH}"
ENV RUSTUP_HOME="/root/.rustup"
ENV CARGO_HOME="/root/.cargo"
ENV RUST_VERSION=1.84.0
ENV SOLANA_VERSION=2.1.5
ENV ANCHOR_VERSION=0.30.1
ENV NODE_VERSION=18.x

# Install basic dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    sudo \
    pkg-config \
    build-essential \
    libudev-dev \
    libssl-dev \
    python3 \
    python3-pip \
    nodejs \
    npm \
    wget \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION} | bash - \
    && apt-get install -y nodejs \
    && npm install -g yarn

# Setup Rust
RUN rustup default ${RUST_VERSION} \
    && rustup component add rustfmt \
    && rustup component add clippy \
    && rustup target add wasm32-unknown-unknown \
    && rustup target add wasm32-wasip1 \
    && rustup target add x86_64-unknown-linux-gnu

# Install Solana CLI using new installation method
RUN sh -c "$(curl -sSfL https://release.anza.xyz/v${SOLANA_VERSION}/install)" \
    && solana --version || echo "Solana installation failed, but continuing"

# Install Anchor
RUN cargo install --git https://github.com/coral-xyz/anchor avm --locked --force \
    && avm install ${ANCHOR_VERSION} \
    && avm use ${ANCHOR_VERSION}

# Set working directory
WORKDIR /app

# Copy project files
COPY . .

# Install dependencies
RUN yarn install

# Generate keypair for tests
RUN mkdir -p ~/.config/solana \
    && solana-keygen new --no-passphrase -s -o ~/.config/solana/id.json || echo "Keypair generation failed, but continuing"

# Start scripts
COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["help"] 