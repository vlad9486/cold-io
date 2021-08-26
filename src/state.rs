// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use super::{
    proposal::{Proposal, ReadOnce, WriteOnce},
    request::Request,
};

/// The deterministic state machine.
/// It accepts proposals from network and issues requests.
/// All business logic should be implemented inside.
pub trait State<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    /// Some user defined inputs to the state machine
    type Ext;

    /// Randomness needed by the state machine
    type Rng;

    /// In order to preserve determinism, it should be the only input to the state machine.
    fn accept(&mut self, proposal: Proposal<R, W, Self::Ext, Self::Rng>) -> Request;
}
