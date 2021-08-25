// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use cold_io::{Proposal, ProposalKind, Proposer, Request, ConnectionSource, State, ReadOnce, WriteOnce, IoResult};

#[derive(Clone, Copy)]
enum ExampleState<const INITIATOR: bool> {
    Empty,
    Done,
}

impl<const INITIATOR: bool> ExampleState<INITIATOR> {
    fn can_terminate(&self) -> bool {
        matches!(self, &ExampleState::Done)
    }
}

impl<R, W, const INITIATOR: bool> State<R, W> for ExampleState<INITIATOR>
where
    R: ReadOnce,
    W: WriteOnce,
{
    type ProposalExt = &'static str;

    fn accept_proposal<Rng>(&mut self, proposal: Proposal<Rng, R, W, Self::ProposalExt>) -> Request
    where
        Rng: rand::RngCore,
    {
        use self::ExampleState::*;
        use std::str;

        const ADDRESS: ([u8; 4], u16) = ([127, 0, 0, 1], 8224);

        log::info!(
            "{}, {}",
            if INITIATOR { "initiator" } else { "responder" },
            proposal
        );

        match (*self, proposal.kind) {
            (Empty, ProposalKind::Wake) => {
                if INITIATOR {
                    Request::default().add_connect(ADDRESS)
                } else {
                    Request::default().set_source(ConnectionSource::Port(8224))
                }
            },
            (Empty, ProposalKind::Idle) => Request::default(),
            (Empty, ProposalKind::OnReadable(addr, once)) => {
                if !INITIATOR {
                    let mut buf = [0; 13];
                    if let IoResult::Done { length, .. } = once.read(&mut buf) {
                        let msg = str::from_utf8(&buf[..length]).unwrap();
                        log::info!("{} -> {:?}", addr, msg);
                        *self = Done;
                        Request::default()
                    } else {
                        // we did not done the task, and connection is closed
                        // let's keep the same state
                        Request::default()
                    }
                } else {
                    // just to be explicit in real code, can omit this `drop`
                    // drop the `once` object, will close the reading half of the connection
                    drop(once);
                    Request::default()
                }
            },
            (Empty, ProposalKind::OnWritable(addr, once)) => {
                if INITIATOR {
                    let msg = "hello, world!";
                    if let IoResult::Done { .. } = once.write(msg.as_bytes()) {
                        log::info!("{} <- {:?}", addr, msg);
                        *self = Done;
                        Request::default()
                    } else {
                        // we did not done the task, and connection is closed
                        // let's try connect again
                        Request::default().add_connect(ADDRESS)
                    }
                } else {
                    drop(once);
                    Request::default()
                }
            },
            (Empty, ProposalKind::Custom(_)) => Request::default(),
            (Done, _) => Request::default(),
        }
    }
}

fn main() {
    use std::{thread, time::Duration};

    env_logger::init();

    let r_thread = thread::spawn(move || {
        let mut responder = ExampleState::<false>::Empty;
        let mut proposer = Proposer::new(12345, 8).unwrap();
        while !responder.can_terminate() {
            proposer
                .run(&mut responder, Duration::from_secs(1))
                .unwrap();
        }
    });
    thread::sleep(Duration::from_millis(100));

    let mut initiator = ExampleState::<true>::Empty;
    let mut proposer = Proposer::new(54321, 8).unwrap();
    while !initiator.can_terminate() {
        proposer
            .run(&mut initiator, Duration::from_secs(1))
            .unwrap();
    }

    r_thread.join().unwrap();
}
