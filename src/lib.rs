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

mod proposer_error;
pub use self::proposer_error::ProposerError;

mod managed_stream;
mod marked_stream;

mod time;
pub use self::time::TimeTracker;

mod stream_registry;
