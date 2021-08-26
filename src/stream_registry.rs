// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{
    collections::{BTreeMap, BTreeSet},
    net::{SocketAddr, IpAddr},
    io,
    time::Duration,
};
use mio::{
    Poll, Events, Token,
    net::{TcpListener, TcpStream},
    Interest,
};

use super::{managed_stream::ManagedStream, request::ConnectionSource, proposer_error::ProposerError};

pub struct StreamRegistry {
    poll: Poll,
    error: ProposerError,
    listener: Option<TcpListener>,
    streams: BTreeMap<SocketAddr, ManagedStream>,
    in_progress: BTreeMap<Token, SocketAddr>,
    blacklist: BTreeSet<IpAddr>,
    last_token: Token,
}

impl StreamRegistry {
    pub const LISTENER: Token = Token(usize::MAX);

    pub fn new() -> Self {
        StreamRegistry {
            poll: Poll::new().expect("cannot use non-blocking io"),
            error: ProposerError::default(),
            listener: None,
            streams: BTreeMap::default(),
            in_progress: BTreeMap::default(),
            blacklist: BTreeSet::default(),
            last_token: Token(0),
        }
    }

    fn allocate_token(&mut self) -> Token {
        let t = self.last_token;
        self.last_token = Token(self.last_token.0 + 1);
        t
    }

    pub fn set_source(&mut self, source: ConnectionSource) {
        if let Some(mut listener) = self.listener.take() {
            // register/reregister/deregister can only fail in case of the bug
            // here and further we should panic in such situation,
            // rather then propagate the error
            self.poll.registry().deregister(&mut listener).expect("bug");
        }

        match source {
            ConnectionSource::None => (),
            ConnectionSource::Port(port) => {
                let mut listener = match TcpListener::bind(([0, 0, 0, 0], port).into()) {
                    Ok(v) => v,
                    Err(e) => {
                        self.error.listen_error = Some((source, e));
                        return;
                    },
                };
                self.poll
                    .registry()
                    .register(&mut listener, Self::LISTENER, Interest::READABLE)
                    .expect("bug");
                self.listener = Some(listener);
            },
        }
    }

    pub fn blacklist_peer(&mut self, addr: SocketAddr) {
        self.blacklist.insert(addr.ip());
        if let Some(stream) = self.streams.remove(&addr) {
            self.poll
                .registry()
                .deregister(stream.borrow_mut().as_mut())
                .expect("bug");
            if let Err(e) = stream.discard() {
                self.error.disconnect_errors.push((addr, e))
            }
        }
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

    pub fn connect_peer(&mut self, addr: SocketAddr) -> Option<Token> {
        if !self.streams.contains_key(&addr) {
            match TcpStream::connect(addr) {
                Ok(stream) => Some(self.register_stream(stream, addr, Interest::WRITABLE)),
                Err(e) => {
                    self.error.connect_errors.push((addr, e));
                    None
                },
            }
        } else {
            None
        }
    }

    pub fn reregister(&mut self) {
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

    pub fn take_stream(&mut self, token: &Token) -> Option<(SocketAddr, &ManagedStream)> {
        let addr = self.in_progress.remove(token)?;
        Some((addr, self.streams.get(&addr).unwrap()))
    }

    pub fn accept(&mut self) -> Option<(SocketAddr, Token)> {
        let listener = self.listener.as_ref()?;
        let (stream, addr) = match listener.accept() {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                return None;
            },
            Err(e) => {
                self.error.accept_error = Some(e);
                return None;
            },
        };
        let token = self.register_stream(stream, addr, Interest::READABLE);
        Some((addr, token))
    }

    pub fn poll(&mut self, events: &mut Events, timeout: Duration) {
        if let Err(e) = self.poll.poll(events, Some(timeout)) {
            if e.kind() != io::ErrorKind::Interrupted {
                self.error.poll_error = Some(e);
            }
        }
    }

    pub fn take_result(&mut self) -> Result<(), ProposerError> {
        self.error.take_result()
    }
}
