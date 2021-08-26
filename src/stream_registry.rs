// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{
    collections::{BTreeMap, BTreeSet},
    net::{SocketAddr, IpAddr},
    io,
};
use mio::{
    Token, Registry,
    net::{TcpListener, TcpStream},
    Interest,
};

use super::{managed_stream::ManagedStream, request::ConnectionSource};

pub struct StreamRegistry {
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

    pub fn set_source(&mut self, registry: &Registry, source: ConnectionSource) -> io::Result<()> {
        if let Some(mut listener) = self.listener.take() {
            // register/reregister/deregister can only fail in case of the bug
            // here and further we should panic in such situation,
            // rather then propagate the error
            registry.deregister(&mut listener).expect("bug");
        }

        match source {
            ConnectionSource::None => Ok(()),
            ConnectionSource::Port(port) => {
                let mut listener = TcpListener::bind(([0, 0, 0, 0], port).into())?;
                registry
                    .register(&mut listener, Self::LISTENER, Interest::READABLE)
                    .expect("bug");
                self.listener = Some(listener);
                Ok(())
            },
        }
    }

    pub fn blacklist_peer(&mut self, registry: &Registry, addr: SocketAddr) -> io::Result<()> {
        self.blacklist.insert(addr.ip());
        if let Some(stream) = self.streams.remove(&addr) {
            registry
                .deregister(stream.borrow_mut().as_mut())
                .expect("bug");
            stream.discard()?;
        }

        Ok(())
    }

    fn register_stream(
        &mut self,
        registry: &Registry,
        stream: TcpStream,
        addr: SocketAddr,
        interests: Interest,
    ) -> Token {
        let token = self.allocate_token();
        let stream = ManagedStream::new(stream, token);
        registry
            .register(stream.borrow_mut().as_mut(), token, interests)
            .expect("bug");
        self.streams.insert(addr, stream);
        self.in_progress.insert(token, addr);
        token
    }

    pub fn connect_peer(&mut self, registry: &Registry, addr: SocketAddr) -> io::Result<Option<Token>> {
        if !self.streams.contains_key(&addr) {
            Ok(Some(self.register_stream(
                registry,
                TcpStream::connect(addr)?,
                addr,
                Interest::WRITABLE,
            )))
        } else {
            Ok(None)
        }
    }

    pub fn reregister(&mut self, registry: &Registry) {
        self.streams.retain(|_, stream| !stream.closed());
        for (addr, stream) in &self.streams {
            if let Some(i) = stream.interests() {
                registry
                    .reregister(stream.borrow_mut().as_mut(), stream.token(), i)
                    .expect("bug");
                self.in_progress.insert(stream.token(), *addr);
            }
        }
        if let Some(listener) = &mut self.listener {
            registry
                .reregister(listener, Self::LISTENER, Interest::READABLE)
                .expect("bug");
        }
    }

    pub fn take_stream(&mut self, token: &Token) -> Option<(SocketAddr, &ManagedStream)> {
        let addr = self.in_progress.remove(token)?;
        Some((addr, self.streams.get(&addr).unwrap()))
    }
    
    pub fn accept(&mut self, registry: &Registry) -> Result<Option<(SocketAddr, Token)>, io::Error> {
        if let Some(listener) = self.listener.as_ref() {
            let (stream, addr) = listener.accept()?;
            let token = self.register_stream(
                registry,
                stream,
                addr,
                Interest::READABLE,
            );
            Ok(Some((addr, token)))
        } else {
            Ok(None)
        }
    }
}
