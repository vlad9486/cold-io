// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{net::SocketAddr, collections::BTreeMap};
use cold_io::{Proposal, ProposalKind, Proposer, Request, ConnectionSource, State, ReadOnce, WriteOnce};

#[derive(Default)]
struct ExampleState {
    connections: BTreeMap<SocketAddr, (Option<ReadOnce>, Option<WriteOnce>)>,
    received_terminate: bool,
}

impl ExampleState {
    fn can_terminate(&self) -> bool {
        self.received_terminate && self.connections.is_empty()
    }
}

impl State for ExampleState {
    type ProposalExt = &'static str;

    fn accept_proposal<R>(&mut self, proposal: Proposal<R, Self::ProposalExt>) -> Request
    where
        R: rand::Rng,
    {
        let empty_request = Request::default();

        log::info!("{}", proposal);

        match proposal.kind {
            ProposalKind::Wake => empty_request.set_source(ConnectionSource::Port(8224)),
            ProposalKind::Idle => {
                if self.received_terminate {
                    if let Some((&addr, _)) = self.connections.iter().next() {
                        log::info!("will disconnect: {}", addr);
                        if let Some((reader, writer)) = self.connections.remove(&addr) {
                            reader.map(ReadOnce::discard);
                            writer.map(WriteOnce::discard);
                        }
                        return empty_request;
                    }
                }
                empty_request
            },
            ProposalKind::OnReadable(addr, once) => {
                let (r, _) = self.connections.entry(addr).or_default();
                *r = Some(once);
                empty_request
            },
            ProposalKind::OnWritable(addr, once) => {
                let (_, w) = self.connections.entry(addr).or_default();
                *w = Some(once);
                empty_request
            },
            ProposalKind::Custom("terminate") => {
                self.received_terminate = true;
                empty_request
            },
            ProposalKind::Custom(_) => empty_request,
        }
    }
}

#[derive(Default)]
struct ClientState {
    connected: bool,
    writer: Option<WriteOnce>,
    reader: Option<ReadOnce>,
}

impl State for ClientState {
    type ProposalExt = &'static str;

    fn accept_proposal<R>(&mut self, proposal: Proposal<R, Self::ProposalExt>) -> Request
    where
        R: rand::Rng,
    {
        match proposal.kind {
            ProposalKind::OnWritable(_, once) => {
                self.writer = Some(once);
            },
            ProposalKind::OnReadable(_, once) => {
                self.reader = Some(once);
            },
            _ => (),
        }

        if self.connected {
            Request::default()
        } else {
            self.connected = true;
            Request::default().add_connect(([127, 0, 0, 1], 8224))
        }
    }
}

fn main() {
    use std::{
        thread,
        time::Duration,
        sync::{
            Arc,
            atomic::{Ordering, AtomicBool},
        },
    };

    env_logger::init();

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::Relaxed))
            .expect("cannot set ctrlc handler");
    }

    let keep_clients = Arc::new(AtomicBool::new(true));
    let clients = {
        let keep_clients = keep_clients.clone();
        let running = running.clone();
        thread::spawn(move || {
            let mut clients = Vec::new();
            while keep_clients.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(1));

                if running.load(Ordering::Relaxed) {
                    let client = ClientState::default();
                    let client_proposer = Proposer::new(12345, 8).unwrap();
                    clients.push((client, client_proposer));
                }
                for (client, client_proposer) in &mut clients {
                    client_proposer
                        .run(client, Duration::from_millis(100))
                        .unwrap();
                }
            }
        })
    };

    let mut server = ExampleState::default();
    let mut proposer = Proposer::new(12345, 8).unwrap();
    while !server.can_terminate() {
        let running = running.load(Ordering::Relaxed);

        if !running {
            server.accept_proposal(Proposal::custom(rand::thread_rng(), "terminate"));
        }

        proposer
            .run(&mut server, Duration::from_millis(500))
            .unwrap();
    }

    keep_clients.store(false, Ordering::Relaxed);
    clients.join().unwrap();
}
