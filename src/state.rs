// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use super::{proposal::Proposal, request::Request};

/// The deterministic state machine.
/// It accepts proposals from network and issues requests.
/// All business logic should be implemented inside.
pub trait State {
    /// Some user defined inputs to the state machine
    type ProposalExt;

    /// In order to preserve determinism, it should be the only input to the state machine.
    fn accept_proposal<R>(&mut self, proposal: Proposal<R, Self::ProposalExt>) -> Request
    where
        R: rand::Rng;
}
