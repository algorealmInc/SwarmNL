# Changes for Drone Swarm Integration

Branch: `feature/drone-swarm-telemetry-tuning`
Parent repo: https://github.com/algorealmInc/SwarmNL

---

## Why This Fork Exists

The `ds-swarm` crate in the `nephilim` drone swarm project uses SwarmNL as its
networking backbone.  The original library was designed for general-purpose
decentralised applications and was not tuned for high-frequency telemetry
workloads.  Three concrete problems blocked the integration:

1. The response-polling loop in `recv_from_network` slept for a **hardcoded 3 s**
   between retries, making every gossip publish call wait up to 30 s.  At a 200 ms
   gossip interval this is two orders of magnitude too slow.

2. The internal network event queue had a **hardcoded capacity of 300** elements.
   Fifty drones gossiping at 200 ms each generate ~250 events/s network-wide.  Under
   partition-recovery bursts the queue would overflow and silently drop events.

3. The eventual-consistency replication background task slept for the **hardcoded
   constant `SYNC_WAIT_TIME` (5 s)** even when a shorter `sync_wait_time` was
   provided via `ReplNetworkConfig::Custom`.  The user-supplied value was stored but
   never read by the sleep call — a silent bug.

---

## Changes

### 1  Configurable `recv_from_network` timeout  (`src/core/prelude.rs`, `src/core/mod.rs`)

**What changed**

Replaced the two hardcoded constants `NETWORK_READ_TIMEOUT = 30` and
`TASK_SLEEP_DURATION = 3` (seconds) with two configurable values stored on
`NetworkInfo`:

| Field | Default | Meaning |
|---|---|---|
| `network_recv_max_polls` | 10 | retries before `NetworkReadTimeout` |
| `network_recv_poll_interval_ms` | 3 000 ms | sleep between retries |

Effective ceiling: `max_polls × poll_interval_ms` ms (default: 30 s, unchanged).

**New API**

```rust
CoreBuilder::with_network_timeout(max_polls: usize, poll_interval_ms: u64) -> Self
```

**Recommended setting for drone swarm**

```rust
// 50 polls × 100 ms = 5 s ceiling, 100 ms granularity
builder.with_network_timeout(50, 100)
```

**Also fixed**: the original `recv_from_network` held the `stream_response_buffer`
`MutexGuard` across the `sleep` call, blocking other tasks from writing responses
into the buffer.  The guard is now dropped explicitly before sleeping.

**Why this is safe to upstream**: purely additive — defaults are identical to the
old behaviour.

---

### 2  Configurable event queue capacity  (`src/core/prelude.rs`, `src/core/mod.rs`)

**What changed**

`DataQueue` previously used a hardcoded capacity of 300 elements (via
`MAX_QUEUE_ELEMENTS`) that could not be adjusted at runtime.  The struct now
stores a `capacity: usize` field set at construction time.

Added `DataQueue::with_capacity(capacity: usize) -> Self`.  The existing
`DataQueue::new()` retains its default of 300 (via `DEFAULT_EVENT_QUEUE_CAPACITY`).

**New API**

```rust
CoreBuilder::with_event_queue_capacity(capacity: usize) -> Self
```

**Recommended setting for drone swarm**

```rust
// 50 drones × ~20 events/s burst = 1 000 headroom
builder.with_event_queue_capacity(2000)
```

**Why this is safe to upstream**: purely additive — defaults are identical to the
old behaviour.

---

### 3  Fix `sync_wait_time` ignored in eventual-consistency loop  (`src/core/replication.rs`)

**What changed**

`ReplicaBufferQueue::sync_with_eventual_consistency` ended each iteration with:

```rust
tokio::time::sleep(Duration::from_secs(Self::SYNC_WAIT_TIME)).await;
```

`Self::SYNC_WAIT_TIME` is the constant `5`.  When a caller configured
`ReplNetworkConfig::Custom { sync_wait_time: 1, .. }` the shorter interval was
stored in the config but never applied — the loop always slept 5 s.

**Fix**: read `sync_wait_time` from `self.config` before the sleep, matching the
pattern already used for `data_aging_period` in the same function:

```rust
let sync_wait_time = match self.config {
    ReplNetworkConfig::Default => Self::SYNC_WAIT_TIME,
    ReplNetworkConfig::Custom { sync_wait_time, .. } => sync_wait_time,
};
tokio::time::sleep(Duration::from_secs(sync_wait_time)).await;
```

**Why this is safe to upstream**: bug fix — `ReplNetworkConfig::Default` behaviour
is unchanged; `Custom` users now get the value they asked for.

---

## What Was Not Changed

- Transport layer (TCP/QUIC).  The `ds-sim` crate handles simulation by assigning
  unique localhost ports to each `SwarmNode` instance and having them dial a
  known seed node directly.  No memory transport is required.
- Gossipsub configuration (fanout, heartbeat intervals) — left at libp2p defaults.
  These can be tuned via `CoreBuilder::with_gossipsub(GossipsubConfig::Custom {...})`
  if propagation latency becomes an issue at >50 nodes.
- Sharding and strong-consistency replication — not used by `ds-swarm`.

---

## Upstream PR Candidates

All three changes are general-purpose improvements with no drone-specific logic.
They are each good upstream PR candidates:

- **PR 1** — "Make recv_from_network polling interval and retry count configurable"
- **PR 2** — "Make DataQueue event-queue capacity configurable at build time"
- **PR 3** — "Fix: ReplNetworkConfig::Custom sync_wait_time ignored in eventual-consistency loop"

---

## How to Point `ds-swarm` at This Fork

```toml
# ds-swarm/Cargo.toml
[dependencies]
swarm-nl = {
  git = "https://github.com/<your-fork>/SwarmNL",
  branch = "feature/drone-swarm-telemetry-tuning"
}
```
