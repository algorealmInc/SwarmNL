#!/bin/bash

# Used for the Docker container to run both nodes

# Run the first command in the background
cargo run --features=first-node &

# Run the second command in the foreground
cargo run --features=second-node