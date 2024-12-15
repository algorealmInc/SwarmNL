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
tmux send-keys -t rust-nodes:0.0 "repl Apples" C-m
sleep 2

# Pane 1 (second node)
tmux send-keys -t rust-nodes:0.1 "repl Oranges" C-m
sleep 2

# Pane 2 (third node)
tmux send-keys -t rust-nodes:0.2 "repl Papayas" C-m
sleep 2

tmux send-keys -t rust-nodes:0.2 "read" C-m

# Attach to the session so you can observe the output
tmux attach-session -t rust-nodes
