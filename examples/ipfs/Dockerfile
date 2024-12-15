# Use a Rust base image
FROM rust:latest

# Install tmux, IPFS, and other utilities
RUN apt-get update && apt-get install -y tmux bash wget && apt-get clean

# Install IPFS
RUN wget https://dist.ipfs.io/go-ipfs/v0.18.1/go-ipfs_v0.18.1_linux-arm64.tar.gz && \
    tar -xvzf go-ipfs_v0.18.1_linux-arm64.tar.gz && \
    mv go-ipfs/ipfs /usr/local/bin/ipfs && \
    rm -rf go-ipfs go-ipfs_v0.18.1_linux-arm64.tar.gz

# Initialize IPFS
RUN ipfs init

# Set the working directory
WORKDIR /usr/src/swarmnl-eventual-consistency-replication-demo

# Copy the Rust project into the container
COPY . .

# Prebuild dependencies to speed up the runtime builds
RUN cargo build --release --features=first-node && \
    cargo build --release --features=second-node && \
    cargo build --release --features=third-node

# Copy the tmux startup script
COPY run_nodes.sh /run_nodes.sh
RUN chmod +x /run_nodes.sh

# Start IPFS daemon in the background and then tmux script
CMD ["bash", "-c", "ipfs daemon & /run_nodes.sh"]
