#!/bin/bash

# Start tmux session
tmux new-session -d -s rust-nodes "cargo run --features=first-node"
tmux split-window -h "cargo run --features=second-node"
tmux split-window -v "cargo run --features=third-node"

# Arrange panes for better layout
tmux select-layout tiled

# Give the nodes some time to start
echo "Waiting for all three nodes to connect..."
sleep 60

# Send commands to each pane
# Pane 0 (first node)
tmux send-keys -t rust-nodes:0.0 "shard 150 song --> Give It Away" C-m
sleep 2

# Pane 1 (second node)
tmux send-keys -t rust-nodes:0.1 "shard 250 song --> Under the Bridge" C-m
sleep 2

# Pane 2 (third node)
tmux send-keys -t rust-nodes:0.2 "shard 55 song --> I Could Have Lied" C-m
sleep 2

tmux send-keys -t rust-nodes:0.0 "shard 210 song --> Castles Made of Sand" C-m
sleep 2
tmux send-keys -t rust-nodes:0.1 "shard 30 song --> Amazing Grace" C-m
sleep 2
tmux send-keys -t rust-nodes:0.2 "shard 255 song --> Hallelujah" C-m
sleep 2

# Read and fetch commands
tmux send-keys -t rust-nodes:0.2 "read" C-m
tmux send-keys -t rust-nodes:0.1 "fetch 210 song" C-m
tmux send-keys -t rust-nodes:0.0 "fetch 50 song" C-m

# Attach to the session so you can observe the output
tmux attach-session -t rust-nodes
