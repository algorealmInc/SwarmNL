# Use a Rust base image
FROM rust:latest

# Install tmux and other utilities
RUN apt-get update && apt-get install -y tmux bash && apt-get clean

# Set the working directory
WORKDIR /usr/src/swarmnl-peer-cloning-replication-demo

# Copy the Rust project into the container
COPY . .

# Prebuild Rust binary to speed up the runtime builds
RUN cargo build --release

# Copy the tmux startup script
COPY run_nodes.sh /run_nodes.sh
RUN chmod +x /run_nodes.sh

# Start tmux with the script
CMD ["/run_nodes.sh"]
