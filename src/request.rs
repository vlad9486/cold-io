// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{net::SocketAddr, mem, ops::AddAssign, fmt};
use smallvec::SmallVec;

/// The proposer will perform requests sequentially.
/// First it setup source, then blacklists and then disconnect.
#[derive(Default, Debug)]
pub struct Request {
    source: Option<ConnectionSource>,
    blacklist: SmallVec<[SocketAddr; 4]>,
    connect: SmallVec<[SocketAddr; 8]>,
}

impl Request {
    pub fn set_source(self, source: ConnectionSource) -> Self {
        let mut s = self;
        s.source = Some(source);
        s
    }

    pub fn add_to_blacklist<A>(self, addr: A) -> Self
    where
        A: Into<SocketAddr>,
    {
        let mut s = self;
        s.blacklist.push(addr.into());
        s
    }

    pub fn add_batch_to_blacklist<I>(self, batch: I) -> Self
    where
        I: IntoIterator<Item = SocketAddr>,
    {
        let mut s = self;
        s.blacklist.extend(batch);
        s
    }

    pub fn add_connect<A>(self, addr: A) -> Self
    where
        A: Into<SocketAddr>,
    {
        let mut s = self;
        s.connect.push(addr.into());
        s
    }

    pub fn add_batch_connect<I>(self, batch: I) -> Self
    where
        I: IntoIterator<Item = SocketAddr>,
    {
        let mut s = self;
        s.connect.extend(batch);
        s
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_none() && self.blacklist.is_empty() && self.connect.is_empty()
    }

    pub fn take_new_source(&mut self) -> Option<ConnectionSource> {
        self.source.take()
    }

    pub fn take_blacklist(&mut self) -> impl Iterator<Item = SocketAddr> {
        mem::take(&mut self.blacklist).into_iter()
    }

    pub fn take_connects(&mut self) -> impl Iterator<Item = SocketAddr> {
        mem::take(&mut self.connect).into_iter()
    }
}

impl AddAssign<Request> for Request {
    fn add_assign(&mut self, rhs: Request) {
        let Request {
            source,
            mut blacklist,
            mut connect,
        } = rhs;
        #[allow(clippy::suspicious_op_assign_impl)]
        if self.source.is_none() && source.is_some() {
            self.source = source;
        }
        self.blacklist.append(&mut blacklist);
        self.connect.append(&mut connect);
    }
}

/// Choose how the proposer will listen incoming connections
#[derive(Debug, Clone, Copy)]
pub enum ConnectionSource {
    /// No incoming connections allowed
    None,
    /// Listen at port
    Port(u16),
}

impl fmt::Display for ConnectionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionSource::None => write!(f, "none"),
            ConnectionSource::Port(port) => write!(f, "port({})", port),
        }
    }
}
