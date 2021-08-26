// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{time::Instant, marker::PhantomData};
use super::{
    state::State,
    proposal::{Proposal, ProposalKind, ReadOnce, WriteOnce},
    request::Request,
};

pub struct TimeTracker<Rngs, S, R, W>
where
    Rngs: Iterator<Item = S::Rng>,
    S: State<R, W>,
    R: ReadOnce,
    W: WriteOnce,
{
    last: Instant,
    rngs: Rngs,
    state: S,
    phantom_data: PhantomData<(R, W)>,
}

impl<Rngs, S, R, W> AsMut<S> for TimeTracker<Rngs, S, R, W>
where
    Rngs: Iterator<Item = S::Rng>,
    S: State<R, W>,
    R: ReadOnce,
    W: WriteOnce,
{
    fn as_mut(&mut self) -> &mut S {
        &mut self.state
    }
}

impl<Rngs, S, R, W> TimeTracker<Rngs, S, R, W>
where
    Rngs: Iterator<Item = S::Rng>,
    S: State<R, W>,
    R: ReadOnce,
    W: WriteOnce,
{
    pub fn new(rngs: Rngs, state: S) -> Self {
        TimeTracker {
            last: Instant::now(),
            rngs,
            state,
            phantom_data: PhantomData,
        }
    }

    pub fn send(&mut self, kind: ProposalKind<R, W, S::Ext>) -> Request {
        use std::mem;

        let last = mem::replace(&mut self.last, Instant::now());
        let proposal = Proposal {
            rng: self.rngs.next().unwrap(),
            elapsed: last.elapsed(),
            kind,
        };

        self.state.accept(proposal)
    }
}
