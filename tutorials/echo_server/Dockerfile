# Use the official Rust image as a base
FROM rust:latest

# Set the working directory inside the container
WORKDIR /usr/src/swarmnl-echo-server

# Copy the Cargo.toml and Cargo.lock files to leverage Docker cache
COPY Cargo.toml Cargo.lock ./
COPY . .

# Run the program and pass the "hello world" to echo 
RUN cargo build
CMD ["cargo", "run"]