# gdrollback
Rollback networking extension for Godot 4 based loosely on the great https://gitlab.com/snopek-games/godot-rollback-netcode

WARNING: This extension is still in development and parts of it
are likely to break or panic if not used correctly in
undocumented ways. You have been warned

## Build

Make sure you have rust installed via https://rustup.rs/

Once installed, build the project with `cargo build; cargo build --release`

## Installation

Create a .gdextension file that contains something like the
following:

```toml
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.1
reloadable = true

[libraries]
windows.debug.x86_64 = "res://{PATH TO THIS REPO}/target/debug/gdrollback.dll"
windows.release.x86_64 = "res://{PATH TO THIS REPO}/target/release/gdrollback.dll"
```

## High Level Overview

This extension is a rollback networking system for Godot 4
which enables responsive peer to peer networking over udp.
The core system is the SyncManager which is registered as a
singleton autoload when the extension is first added.


The SyncManager has three stages of operation:
1. Lobby - where peers connect to a known host and port port
   forwarding details are gossiped between them. Negotiation
   for when to start the game is also managed here.
2. Play - once scheduled, the game starts moving the
   SyncManager into the play state where `networked` nodes
   are ticked and reloaded based on received input. During
   play, all inputs, debug state, and other related details
   are logged to a sqlite database for later replay.
3. Replay - If instead of connecting and scheduling a start,
   the user selects a replay file, the SyncManager will
   collect inputs from the replay and play them back in a
   predictable manner.

## Lobby

The following is available from the SyncManager in Lobby mode:

### `@signal start_scheduled()`

Emitted when the game has been scheduled to start after all
peers have declared themselves ready.

### `@signal connected(id: String)`

Emitted when a peer has connected either by direct
connection, or by being gossiped from an already connected
peer.

### `@signal started()`

Emitted when the game has started. A scene with `networked`
nodes should be initialized at this point.

### `host(port: int)`

Starts listening for connections on the given port. Ideally
this would be done on a machine and with a port that has
been port forwarded or otherwise exposed in some way.

Note: This is not really a host per se, just an exposed
user. Rollback networking via this extension is peer to
peer, so no one player has more responsibility or advantage
than any other.

### `join(address: String, port: int)`

Attempts to connect to the given address and port.

### `update_ready(ready: bool)`

Declares that this client is ready to start the game. When
executed, the SyncManager will send a ready signal to every
connected peer. If all peers are ready, the peer with the
smallest GUID will send a message scheduling the start of
the game. When the game start message is received, and the
appropriate amount of time has been waited, the SyncManager
will raise the `started` signal and move into the play
state.

### `replay(replay_path: String)`

If a valid replay file is passed, the SyncManager will load
it and move into the replay state which operates very
similarly to the play state, but where inputs are read from
the replay file instead of received from the network or
local machine.

## Play

### InputManager

The SyncManager expects a global autoload called
`InputManager` with a method `networked_input` on it which
returns the input for that frame. Currently this input is
required to have a specific format (check 
`src/play_stage/input.rs`), but this will likely be relaxed
in a future update.

### `networked` Nodes

During play/replay modes, any nodes that are a part of the
`networked` are expected to be managed by the SyncManager
via specific methods. The following functions have special
purposes:

#### `networked_process() -> {state}`

Called every frame by the SyncManager and is responsible for
updating the state of the node by one tick and should return
whatever state is necessary to reload the node back to this
frame in the future.

#### `load_state(state)`

Called by the SyncManager whenever an input from an earlier
frame is received. This is used to reload the state back to
the frame before the received input in preparation for
simulating forward to the current frame again. The state
passed to this function is the same state that was returned
from `networked_process`

#### `networked_preprocess()`

Called by the SyncManager before `networked_process`. Useful
for maintaining state at the start of a frame.

#### `log_state() -> {state}`

Called by the SyncManager at the end of the frame to log a
dictionary of state to the replay database for debug
purposes. This state is also used by hashing it to verify
that a desync has not occurred.

#### `networked_despawn()`

Called by the SyncManager when a node has been despawned.
Convenient for cleaning up any resources and resetting the
node to a default state for reuse.

#### `networked_spawn(state)`

Called by the SyncManager when a node is spawned. The state
passed is an argument that was passed to the SyncManager's
`spawn` method and is used to initialize the node
repeatably.

### Play State Functions

The following functions are available when the SyncManager
is in play/replay state:

#### `local_id() -> String`

Returns the GUID of the local machine.

#### `remote_ids() -> Array<String>`

Returns an array of GUIDs for all connected peers.

#### `ids() -> Array<String>`

Returns an array of all peer GUIDs including the local machine.

#### `is_leader() -> bool`

Returns a boolean indicating if this machine was elected
leader.

#### `input(id: String) -> Input`

Returns the input for the given peer (or local machine) for
the currently simulated frame.

#### `advantage() -> float`

Returns the average advantage for this machine over all
connected peers in terms of frames. This value is minimized
automatically by dropping frames when it is determined that
a given peer is significantly ahead.

#### `despawn(node: Node)`

Despawns the given node. This is necessary to ensure that
nodes are despawned correctly across rollbacks.

#### `spawn(name: String, parent: Node, scene: PackedScene, data: Dictionary) -> Node`

Spawns a new node of the given scene under the given parent
with the given name. Data is passed to `networked_spawn`.
This method is necessary to ensure that nodes are despawned
and spawned correctly across rollbacks.

#### `log(event: String)`

Logs an event to the replay database. Useful for debugging
purposes.
