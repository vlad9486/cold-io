# Cold IO

It is the wrapper over `mio`. Allows to write a network application as a state machine to simplify testing.

## Proposer

The core entity is `Proposer` it working with the state machine that user implemented.

The `Proposer` provides the state machine a sequence of `Proposal`. For each `Proposal` state machine can return some `Request`.

## Proposal

It containing random number generator, elapsed time from previous proposal and one of the following messages:

* Wake - The first message that `Proposer` sends to the state machine. It needed for the state machine to provide a first request.
* Idle - The message that means nothing happened during some time.
* OnReadable/OnWritable - Some remote peer is ready to transmit/receive data. With this message a managed stream is provided. This object can be used only once.

## Managed Stream

The state machine receive `ReadOnce` object along with `OnReadable` event. The state machine can read it, or discard, or store for further use. The proposer will not send another `ReadOnce` until previous did not consumed. It can be dropped, but it is a wast of time, because proposer will resend it. If the state machine discard the object, the `TcpStream` will be closed. If the state machine read the object, it will know how many bytes was read, and whether there will be more.

The state machine receive `WriteOnce` object along with `OnWritable` event. It is very similar to `ReadOnce`. Except it is possible to explicitly close the connection after write. So it has `write` method, and `write_last` which means we don't want to write more and the connection will be closed.

## Request

There are following elemental requests:

* Source of incoming connections. It can be a port, or nothing. It is planned to receive incoming connections from another thread.
* Blacklist a peer or a batch of peers.
* Connect to a peer or to a batch of peers.
