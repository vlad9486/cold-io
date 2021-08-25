// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{time::Duration, net::SocketAddr, fmt};

pub trait ReadOnce {
    fn read(self, buf: &mut [u8]) -> IoResult;
}

pub trait WriteOnce {
    fn write(self, data: &[u8]) -> IoResult;
}

pub enum IoResult {
    Closed,
    Done {
        length: usize,
        will_close: bool,
    },
}

/// The proposal is the input to the state machine.
/// It provides timer, io, random number generator, and user-defined data.
pub struct Proposal<Rng, R, W, E> {
    pub rng: Rng,
    pub elapsed: Duration,
    pub kind: ProposalKind<R, W, E>,
}

impl<Rng, R, W, E> Proposal<Rng, R, W, E> {
    pub fn custom(rng: Rng, ext: E) -> Self {
        Proposal {
            rng,
            elapsed: Duration::ZERO,
            kind: ProposalKind::Custom(ext),
        }
    }
}

pub enum ProposalKind<R, W, E> {
    /// Wake the state machine, useful if the state machine
    /// needs to request something before it receives any event
    Wake,
    /// Nothing happened during a time quant
    Idle,
    /// The remote peer can provide data.
    OnReadable(SocketAddr, R),
    /// The remote peer can accept data.
    OnWritable(SocketAddr, W),
    /// User-defined
    Custom(E),
}

impl<Rng, R, W, E> fmt::Display for Proposal<Rng, R, W, E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elapsed: {:?}, {}", self.elapsed, self.kind)
    }
}

impl<R, W, E> fmt::Display for ProposalKind<R, W, E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProposalKind::Wake => write!(f, "wake"),
            ProposalKind::Idle => write!(f, "idle..."),
            ProposalKind::OnReadable(addr, _) => write!(f, "local peer can read from {}", addr),
            ProposalKind::OnWritable(addr, _) => write!(f, "local peer can write to {}", addr),
            ProposalKind::Custom(ext) => write!(f, "{}", ext),
        }
    }
}
