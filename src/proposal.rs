// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{time::Duration, net::SocketAddr, fmt};
use super::managed_stream::{ReadOnce, WriteOnce};

/// The proposal is the input to the state machine.
/// It provides timer, io, random number generator, and user-defined data.
pub struct Proposal<R, E>
where
    R: rand::Rng,
{
    pub rng: R,
    pub elapsed: Duration,
    pub kind: ProposalKind<E>,
}

impl<R, E> Proposal<R, E>
where
    R: rand::Rng,
{
    pub fn custom(rng: R, ext: E) -> Self {
        Proposal {
            rng,
            elapsed: Duration::ZERO,
            kind: ProposalKind::Custom(ext),
        }
    }
}

pub enum ProposalKind<E> {
    /// Wake the state machine, useful if the state machine
    /// needs to request something before it receives any event
    Wake,
    /// Nothing happened during a time quant
    Idle,
    /// The remote peer can provide data.
    OnReadable(SocketAddr, ReadOnce),
    /// The remote peer can accept data.
    OnWritable(SocketAddr, WriteOnce),
    /// User-defined
    Custom(E),
}

impl<R, E> fmt::Display for Proposal<R, E>
where
    R: rand::Rng,
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elapsed: {:?}, {}", self.elapsed, self.kind)
    }
}

impl<E> fmt::Display for ProposalKind<E>
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
