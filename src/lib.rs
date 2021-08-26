// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

#![forbid(unsafe_code)]

mod state;
pub use self::state::State;

mod request;
pub use self::request::{Request, ConnectionSource};

mod proposal;
pub use self::proposal::{Proposal, ProposalKind, ConnectionId, ReadOnce, WriteOnce, IoResult};

mod proposer;
pub use self::proposer::Proposer;

mod managed_stream;
mod marked_stream;
