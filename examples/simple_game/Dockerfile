# Use the official Rust image as a base
FROM rust:latest

# Set the working directory inside the container
WORKDIR /usr/src/swarmnl-simple-game-demo

# Copy the Cargo.toml and Cargo.lock files to leverage Docker cache
COPY Cargo.toml Cargo.lock ./
COPY . .

# Build the Rust crate
RUN cargo build --release

# Run the script to start both nodes
COPY run_both_nodes.sh .
RUN chmod +x run_both_nodes.sh 

CMD ["./run_both_nodes.sh"]