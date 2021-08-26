// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::time::Duration;
use mio::Events;

use super::{
    request::Request,
    managed_stream::{TcpReadOnce, TcpWriteOnce},
    state::State,
    proposal::{ProposalKind, ConnectionId},
    time::TimeTracker,
    stream_registry::StreamRegistry,
    proposer_error::ProposerError,
};

/// The proposer serves the state's requests and provides network events to it.
pub struct Proposer {
    started: bool,
    request: Request,
    events: Events,
    id: u16,
    stream_registry: StreamRegistry,
}

impl Proposer {
    /// Set the seed for the random number generator.
    pub fn new(id: u16, events_capacity: usize) -> Self {
        Proposer {
            started: false,
            request: Request::default(),
            events: Events::with_capacity(events_capacity),
            id,
            stream_registry: StreamRegistry::new(),
        }
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
        if !self.started {
            self.started = true;
            self.request += time_tracker.send(ProposalKind::Wake);
            return Ok(());
        }

        if let Some(source) = self.request.take_new_source() {
            self.stream_registry.set_source(source);
        }

        for addr in self.request.take_blacklist() {
            self.stream_registry.blacklist_peer(addr);
        }

        self.stream_registry.reregister();

        for addr in self.request.take_connects() {
            if let Some(token) = self.stream_registry.connect_peer(addr) {
                let kind = ProposalKind::Connection {
                    addr,
                    incoming: false,
                    id: ConnectionId {
                        poll_id: self.id,
                        token: token.0 as u16,
                    },
                };
                self.request += time_tracker.send(kind);
            }
        }

        self.stream_registry.poll(&mut self.events, timeout);

        if self.events.is_empty() {
            self.request += time_tracker.send(ProposalKind::Idle);
        }
        for event in self.events.into_iter() {
            if event.token() == StreamRegistry::LISTENER {
                while let Some((addr, token)) = self.stream_registry.accept() {
                    let kind = ProposalKind::Connection {
                        addr,
                        incoming: true,
                        id: ConnectionId {
                            poll_id: self.id,
                            token: token.0 as u16,
                        },
                    };
                    self.request += time_tracker.send(kind);
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

        self.stream_registry.take_result()
    }
}
