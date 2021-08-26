// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{
    time::{Duration, Instant},
    collections::{BTreeMap, BTreeSet},
    net::{SocketAddr, IpAddr},
    io, fmt,
    error::Error,
};
use mio::{
    Poll, Events, Token,
    net::{TcpListener, TcpStream},
    Interest,
};

use super::{
    request::{Request, ConnectionSource},
    managed_stream::{ManagedStream, TcpReadOnce, TcpWriteOnce},
    state::State,
    proposal::{Proposal, ProposalKind, ConnectionId},
};

/// The proposer serves the state's requests and provides network events to it.
pub struct Proposer {
    started: bool,
    request: Request,
    poll: Poll,
    events_capacity: usize,
    events: Events,
    last: Instant,
    id: u16,
    listener: Option<TcpListener>,
    streams: BTreeMap<SocketAddr, ManagedStream>,
    in_progress: BTreeMap<Token, SocketAddr>,
    blacklist: BTreeSet<IpAddr>,
    last_token: Token,
}

impl Proposer {
    const LISTENER: Token = Token(usize::MAX);

    /// Set the seed for the random number generator.
    pub fn new(id: u16, events_capacity: usize) -> io::Result<Self> {
        let poll = Poll::new()?;

        Ok(Proposer {
            started: false,
            request: Request::default(),
            poll,
            events_capacity,
            events: Events::with_capacity(events_capacity),
            last: Instant::now(),
            id,
            listener: None,
            streams: BTreeMap::default(),
            in_progress: BTreeMap::default(),
            blacklist: BTreeSet::default(),
            last_token: Token(0),
        })
    }

    fn allocate_token(&mut self) -> Token {
        let t = self.last_token;
        self.last_token = Token(self.last_token.0 + 1);
        t
    }

    fn send_proposal<S>(
        &mut self,
        rng: S::Rng,
        state: &mut S,
        kind: ProposalKind<TcpReadOnce, TcpWriteOnce, S::Ext>,
    ) where
        S: State<TcpReadOnce, TcpWriteOnce>,
    {
        use std::mem;

        let last = mem::replace(&mut self.last, Instant::now());
        let proposal = Proposal {
            rng,
            elapsed: last.elapsed(),
            kind,
        };

        self.request += state.accept(proposal);
    }

    fn set_source(&mut self, source: ConnectionSource) -> io::Result<()> {
        if let Some(mut listener) = self.listener.take() {
            // register/reregister/deregister can only fail in case of the bug
            // here and further we should panic in such situation,
            // rather then propagate the error
            self.poll.registry().deregister(&mut listener).expect("bug");
        }

        match source {
            ConnectionSource::None => Ok(()),
            ConnectionSource::Port(port) => {
                let mut listener = TcpListener::bind(([0, 0, 0, 0], port).into())?;
                self.poll
                    .registry()
                    .register(&mut listener, Self::LISTENER, Interest::READABLE)
                    .expect("bug");
                self.listener = Some(listener);
                Ok(())
            },
        }
    }

    fn disconnect_peer(&mut self, addr: SocketAddr) -> io::Result<()> {
        if let Some(stream) = self.streams.remove(&addr) {
            self.poll
                .registry()
                .deregister(stream.borrow_mut().as_mut())
                .expect("bug");
            stream.discard()?;
        }

        Ok(())
    }

    fn register_stream(
        &mut self,
        stream: TcpStream,
        addr: SocketAddr,
        interests: Interest,
    ) -> Token {
        let token = self.allocate_token();
        let stream = ManagedStream::new(stream, token);
        self.poll
            .registry()
            .register(stream.borrow_mut().as_mut(), token, interests)
            .expect("bug");
        self.streams.insert(addr, stream);
        self.in_progress.insert(token, addr);
        token
    }

    fn connect_peer(&mut self, addr: SocketAddr) -> io::Result<Option<Token>> {
        if !self.streams.contains_key(&addr) {
            Ok(Some(self.register_stream(
                TcpStream::connect(addr)?,
                addr,
                Interest::WRITABLE,
            )))
        } else {
            Ok(None)
        }
    }

    fn reregister(&mut self) {
        self.streams.retain(|_, stream| !stream.closed());
        for (addr, stream) in &self.streams {
            if let Some(i) = stream.interests() {
                self.poll
                    .registry()
                    .reregister(stream.borrow_mut().as_mut(), stream.token(), i)
                    .expect("bug");
                self.in_progress.insert(stream.token(), *addr);
            }
        }
        if let Some(listener) = &mut self.listener {
            self.poll
                .registry()
                .reregister(listener, Self::LISTENER, Interest::READABLE)
                .expect("bug");
        }
    }

    fn take_events(&mut self) -> Events {
        std::mem::replace(
            &mut self.events,
            Events::with_capacity(self.events_capacity),
        )
    }

    /// Run the single iteration
    pub fn run<Rngs, S>(
        &mut self,
        rngs: &mut Rngs,
        state: &mut S,
        timeout: Duration,
    ) -> Result<(), ProposerError>
    where
        Rngs: Iterator<Item = S::Rng>,
        S: State<TcpReadOnce, TcpWriteOnce>,
    {
        if self.started {
            self.run_inner(rngs, state, timeout)
        } else {
            self.started = true;
            self.send_proposal(rngs.next().unwrap(), state, ProposalKind::Wake);
            Ok(())
        }
    }

    fn run_inner<Rngs, S>(
        &mut self,
        rngs: &mut Rngs,
        state: &mut S,
        timeout: Duration,
    ) -> Result<(), ProposerError>
    where
        Rngs: Iterator<Item = S::Rng>,
        S: State<TcpReadOnce, TcpWriteOnce>,
    {
        let mut error = ProposerError::default();

        if let Some(source) = self.request.take_new_source() {
            if let Err(e) = self.set_source(source) {
                error.listen_error = Some((source, e));
            }
        }

        for addr in self.request.take_blacklist() {
            self.blacklist.insert(addr.ip());
            if let Err(e) = self.disconnect_peer(addr) {
                error.disconnect_errors.push((addr, e));
            }
        }

        self.reregister();

        for addr in self.request.take_connects() {
            match self.connect_peer(addr) {
                Err(e) => error.connect_errors.push((addr, e)),
                Ok(None) => (),
                Ok(Some(token)) => {
                    let kind = ProposalKind::Connection {
                        addr,
                        incoming: true,
                        id: ConnectionId {
                            poll_id: self.id,
                            token: token.0 as u16,
                        },
                    };
                    self.send_proposal(rngs.next().unwrap(), state, kind);
                },
            }
        }

        match self.poll.poll(&mut self.events, Some(timeout)) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
            Err(e) => {
                let _ = self.take_events();
                error.poll_error = Some(e);
                return Err(error);
            },
        }

        let events = self.take_events();
        if events.is_empty() {
            self.send_proposal(rngs.next().unwrap(), state, ProposalKind::Idle);
        }
        for event in events.into_iter() {
            if event.token() == Self::LISTENER {
                if let Some(listener) = self.listener.as_mut() {
                    match listener.accept() {
                        Ok((stream, addr)) => {
                            if !self.blacklist.contains(&addr.ip()) {
                                let token = self.register_stream(stream, addr, Interest::READABLE);
                                let kind = ProposalKind::Connection {
                                    addr,
                                    incoming: true,
                                    id: ConnectionId {
                                        poll_id: self.id,
                                        token: token.0 as u16,
                                    },
                                };
                                self.send_proposal(rngs.next().unwrap(), state, kind);
                            }
                        },
                        Err(e) => {
                            error.accept_error = Some(e);
                        },
                    }
                }
            } else if let Some(addr) = self.in_progress.remove(&event.token()) {
                let stream = self.streams.get(&addr).unwrap();
                let id = ConnectionId {
                    poll_id: self.id,
                    token: stream.token().0 as u16,
                };
                let mut pr = Vec::with_capacity(2);
                if event.is_writable() {
                    if let Some(w) = stream.write_once() {
                        pr.push(ProposalKind::OnWritable(id, w));
                        if event.is_write_closed() {
                            stream.set_write_closed();
                        }
                    } else {
                        debug_assert!(false, "mio should not poll for this event");
                    }
                }
                if event.is_readable() {
                    if let Some(r) = stream.read_once() {
                        pr.push(ProposalKind::OnReadable(id, r));
                        if event.is_read_closed() {
                            stream.set_read_closed();
                        }
                    } else {
                        debug_assert!(false, "mio should not poll for this event");
                    }
                }
                for pr in pr {
                    self.send_proposal(rngs.next().unwrap(), state, pr);
                }
            }
        }

        error.into_result()
    }
}

#[derive(Debug, Default)]
pub struct ProposerError {
    listen_error: Option<(ConnectionSource, io::Error)>,
    connect_errors: Vec<(SocketAddr, io::Error)>,
    disconnect_errors: Vec<(SocketAddr, io::Error)>,
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
    pub fn into_result(self) -> Result<(), Self> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.listen_error.is_none()
            && self.connect_errors.is_empty()
            && self.disconnect_errors.is_empty()
            && self.accept_error.is_none()
            && self.poll_error.is_none()
    }
}
