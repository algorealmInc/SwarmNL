# Replication

After configuring your node for replication, you can participate in replication across the network by calling methods exposed to the application layer through [`Core`]:

- [`Core::replicate()`]: Replicates data across replica nodes on the network.
- [`Core::replicate_buffer()`]: Clone a remote node's replica buffer.
- [`Core::join_repl_network()`]: Join a replication network.
- [`Core::leave_repl_network()`]: Exit a replication network.
- Etc.