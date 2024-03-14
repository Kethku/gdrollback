# udp_ext

Crate providing a series of layers on top of simple udp
sockets to provide progressively more extensive guarantees.
Also provides a simple way to author packets for sending
over any of the layers.

Each layer wraps the layer below it. Events from the socket
are retrieved by calling the `pump` function on the socket
which will retrieve and process any messages that have been
received since the last call to `pump`.

## ReliableSocket

The most basic layer resends packets until they have been
properly acknowledged.

## FrameSocket

The next layer up will split packets that are over a maximum
size into multiple subpackets which are then reassembled on
the other side.

## Persistent 

The final layer maintains connections and response times for
each peer. Each connected peer must be assigned a unique ID
which is used to refer to them at this layer.
