// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{time::Duration, net::SocketAddr, io, fmt, error::Error};
use mio::{Poll, Events};
use smallvec::SmallVec;

use super::{
    request::{Request, ConnectionSource},
    managed_stream::{TcpReadOnce, TcpWriteOnce},
    state::State,
    proposal::{ProposalKind, ConnectionId},
    time::TimeTracker,
    stream_registry::StreamRegistry,
};

/// The proposer serves the state's requests and provides network events to it.
pub struct Proposer {
    started: bool,
    request: Request,
    poll: Poll,
    events: Events,
    id: u16,
    stream_registry: StreamRegistry,
}

impl Proposer {
    /// Set the seed for the random number generator.
    pub fn new(id: u16, events_capacity: usize) -> io::Result<Self> {
        let poll = Poll::new()?;

        Ok(Proposer {
            started: false,
            request: Request::default(),
            poll,
            events: Events::with_capacity(events_capacity),
            id,
            stream_registry: StreamRegistry::new(),
        })
    }

    /// Run the single iteration
    pub fn run<Rngs, S>(
        &mut self,
        time_tracker: &mut TimeTracker<Rngs, S, TcpReadOnce, TcpWriteOnce>,
        timeout: Duration,
    ) -> Result<(), ProposerError>
    where
        Rngs: Iterator<Item = S::Rng>,
        S: State<TcpReadOnce, TcpWriteOnce>,
    {
        if self.started {
            self.run_inner(time_tracker, timeout)
        } else {
            self.started = true;
            self.request += time_tracker.send(ProposalKind::Wake);
            Ok(())
        }
    }

    fn run_inner<Rngs, S>(
        &mut self,
        time_tracker: &mut TimeTracker<Rngs, S, TcpReadOnce, TcpWriteOnce>,
        timeout: Duration,
    ) -> Result<(), ProposerError>
    where
        Rngs: Iterator<Item = S::Rng>,
        S: State<TcpReadOnce, TcpWriteOnce>,
    {
        let mut error = ProposerError::default();

        if let Some(source) = self.request.take_new_source() {
            if let Err(e) = self
                .stream_registry
                .set_source(self.poll.registry(), source)
            {
                error.listen_error = Some((source, e));
            }
        }

        for addr in self.request.take_blacklist() {
            if let Err(e) = self
                .stream_registry
                .blacklist_peer(self.poll.registry(), addr)
            {
                error.disconnect_errors.push((addr, e));
            }
        }

        self.stream_registry.reregister(self.poll.registry());

        for addr in self.request.take_connects() {
            match self
                .stream_registry
                .connect_peer(self.poll.registry(), addr)
            {
                Err(e) => error.connect_errors.push((addr, e)),
                Ok(None) => (),
                Ok(Some(token)) => {
                    let kind = ProposalKind::Connection {
                        addr,
                        incoming: false,
                        id: ConnectionId {
                            poll_id: self.id,
                            token: token.0 as u16,
                        },
                    };
                    self.request += time_tracker.send(kind);
                },
            }
        }

        match self.poll.poll(&mut self.events, Some(timeout)) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
            Err(e) => {
                error.poll_error = Some(e);
            },
        }

        if self.events.is_empty() {
            self.request += time_tracker.send(ProposalKind::Idle);
        }
        for event in self.events.into_iter() {
            if event.token() == StreamRegistry::LISTENER {
                loop {
                    match self.stream_registry.accept(self.poll.registry()) {
                        Err(e) if io::ErrorKind::WouldBlock == e.kind() => {
                            break;
                        },
                        Err(e) => {
                            error.accept_error = Some(e);
                            break;
                        },
                        Ok(None) => break,
                        Ok(Some((addr, token))) => {
                            let kind = ProposalKind::Connection {
                                addr,
                                incoming: true,
                                id: ConnectionId {
                                    poll_id: self.id,
                                    token: token.0 as u16,
                                },
                            };
                            self.request += time_tracker.send(kind);
                        },
                    }
                }
            } else if let Some((_, stream)) = self.stream_registry.take_stream(&event.token()) {
                let id = ConnectionId {
                    poll_id: self.id,
                    token: stream.token().0 as u16,
                };
                if event.is_writable() {
                    if let Some(w) = stream.write_once() {
                        if event.is_write_closed() {
                            stream.set_write_closed();
                        }
                        self.request += time_tracker.send(ProposalKind::OnWritable(id, w));
                    } else {
                        debug_assert!(false, "mio should not poll for this event");
                    }
                }
                if event.is_readable() {
                    if let Some(r) = stream.read_once() {
                        if event.is_read_closed() {
                            stream.set_read_closed();
                        }
                        self.request += time_tracker.send(ProposalKind::OnReadable(id, r));
                    } else {
                        debug_assert!(false, "mio should not poll for this event");
                    }
                }
            }
        }

        error.into_result()
    }
}

#[derive(Debug, Default)]
pub struct ProposerError {
    listen_error: Option<(ConnectionSource, io::Error)>,
    connect_errors: SmallVec<[(SocketAddr, io::Error); 8]>,
    disconnect_errors: SmallVec<[(SocketAddr, io::Error); 4]>,
    accept_error: Option<io::Error>,
    poll_error: Option<io::Error>,
}

impl fmt::Display for ProposerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((source, error)) = &self.listen_error {
            write!(f, "failed to listen: {}, error: {}", source, error)?;
        }
        for (addr, error) in &self.connect_errors {
            write!(f, "failed to connect to: {}, error: {}", addr, error)?;
        }
        for (addr, error) in &self.disconnect_errors {
            write!(f, "failed to disconnect from: {}, error: {}", addr, error)?;
        }
        if let Some(error) = &self.accept_error {
            write!(f, "failed to accept a connection, error: {}", error)?;
        }
        if let Some(error) = &self.poll_error {
            write!(f, "failed to poll the events, error: {}", error)?;
        }

        Ok(())
    }
}

impl Error for ProposerError {}

impl ProposerError {
    fn into_result(self) -> Result<(), Self> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }

    fn is_empty(&self) -> bool {
        self.listen_error.is_none()
            && self.connect_errors.is_empty()
            && self.disconnect_errors.is_empty()
            && self.accept_error.is_none()
            && self.poll_error.is_none()
    }
}
