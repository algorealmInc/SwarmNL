# Use the official Rust image as a base
FROM rust:latest

# Set the working directory inside the container
WORKDIR /usr/src/swarmnl-file-sharing-demo

# Copy the Cargo.toml and Cargo.lock files to leverage Docker cache
COPY Cargo.toml Cargo.lock ./
COPY . .

# Build the Rust crate
RUN cargo build --release

# Run the first node
CMD ["cargo", "run", "--release", "--features=first-node"]

# Run the second node after the first one completes
CMD ["cargo", "run", "--release", "--features=second-node"]