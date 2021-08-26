// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{time::Duration, net::SocketAddr, fmt};

pub trait ReadOnce {
    fn read(self, buf: &mut [u8]) -> IoResult;
}

pub trait WriteOnce {
    fn write(self, data: &[u8]) -> IoResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use = "need to know how many bytes was actually read or written"]
pub enum IoResult {
    Closed,
    Done { length: usize, will_close: bool },
}

/// The proposal is the input to the state machine.
/// It provides timer, io, random number generator, and user-defined data.
pub struct Proposal<R, W, Ext, Rng> {
    pub rng: Rng,
    pub elapsed: Duration,
    pub kind: ProposalKind<R, W, Ext>,
}

impl<R, W, Ext, Rng> Proposal<R, W, Ext, Rng> {
    pub fn custom(rng: Rng, ext: Ext) -> Self {
        Proposal {
            rng,
            elapsed: Duration::ZERO,
            kind: ProposalKind::Custom(ext),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId {
    pub poll_id: u16,
    pub token: u16,
}

pub enum ProposalKind<R, W, Ext> {
    /// Wake the state machine, useful if the state machine
    /// needs to request something before it receives any event
    Wake,
    /// Nothing happened during a time quant
    Idle,
    /// New connection
    Connection {
        addr: SocketAddr,
        incoming: bool,
        id: ConnectionId,
    },
    /// The remote peer can provide data.
    OnReadable(ConnectionId, R),
    /// The remote peer can accept data.
    OnWritable(ConnectionId, W),
    /// User-defined
    Custom(Ext),
}

impl<R, W, Ext, Rng> fmt::Display for Proposal<R, W, Ext, Rng>
where
    Ext: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elapsed: {:?}, {}", self.elapsed, self.kind)
    }
}

impl<R, W, Ext> fmt::Display for ProposalKind<R, W, Ext>
where
    Ext: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProposalKind::Wake => write!(f, "wake"),
            ProposalKind::Idle => write!(f, "idle..."),
            ProposalKind::Connection { addr, incoming, id } => {
                if *incoming {
                    write!(f, "new incoming connection: {}, addr: {}", id, addr)
                } else {
                    write!(f, "new outgoing connection: {}, addr: {}", id, addr)
                }
            },
            ProposalKind::OnReadable(id, _) => write!(f, "local peer can read from {}", id),
            ProposalKind::OnWritable(id, _) => write!(f, "local peer can write to {}", id),
            ProposalKind::Custom(ext) => write!(f, "{}", ext),
        }
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}.{:04x}", self.poll_id, self.token)
    }
}
