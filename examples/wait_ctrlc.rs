// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use cold_io::{
    Proposal, ProposalKind, ConnectionId, Proposer, Request, ConnectionSource, State, ReadOnce,
    WriteOnce, TimeTracker,
};

struct ExampleState<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    connections: BTreeMap<ConnectionId, (Option<R>, Option<W>)>,
    received_terminate: bool,
}

impl<R, W> ExampleState<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    fn can_terminate(&self) -> bool {
        self.received_terminate && self.connections.is_empty()
    }
}

impl<R, W> State<R, W> for ExampleState<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    type Ext = &'static str;

    type Rng = ();

    fn accept(&mut self, proposal: Proposal<R, W, Self::Ext, Self::Rng>) -> Request {
        log::info!("{}", proposal);

        match proposal.kind {
            ProposalKind::Wake => Request::default().set_source(ConnectionSource::Port(8224)),
            ProposalKind::Idle => {
                if self.received_terminate {
                    if let Some((&addr, _)) = self.connections.iter().next() {
                        log::info!("will disconnect: {}", addr);
                        self.connections.remove(&addr);
                    }
                }
                Request::default()
            },
            ProposalKind::Connection { .. } => Request::default(),
            ProposalKind::OnReadable(id, once) => {
                let (r, _) = self.connections.entry(id).or_default();
                *r = Some(once);
                Request::default()
            },
            ProposalKind::OnWritable(id, once) => {
                let (_, w) = self.connections.entry(id).or_default();
                *w = Some(once);
                Request::default()
            },
            ProposalKind::Custom("terminate") => {
                self.received_terminate = true;
                Request::default()
            },
            ProposalKind::Custom(_) => Request::default(),
        }
    }
}

struct ClientState<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    connected: bool,
    writer: Option<W>,
    reader: Option<R>,
}

impl<R, W> State<R, W> for ClientState<R, W>
where
    R: ReadOnce,
    W: WriteOnce,
{
    type Ext = &'static str;

    type Rng = ();

    fn accept(&mut self, proposal: Proposal<R, W, Self::Ext, Self::Rng>) -> Request {
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
        iter,
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
                    let client = TimeTracker::new(
                        iter::repeat(()),
                        ClientState {
                            connected: false,
                            reader: None,
                            writer: None,
                        },
                    );
                    let client_proposer = Proposer::new(clients.len() as u16 + 1, 8).unwrap();
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

    let mut server = TimeTracker::new(
        iter::repeat(()),
        ExampleState {
            connections: BTreeMap::default(),
            received_terminate: false,
        },
    );
    let mut proposer = Proposer::new(0, 8).unwrap();
    while !server.as_mut().can_terminate() {
        let running = running.load(Ordering::Relaxed);

        if !running {
            server.send(ProposalKind::Custom("terminate"));
        }

        proposer
            .run(&mut server, Duration::from_millis(500))
            .unwrap();
    }

    keep_clients.store(false, Ordering::Relaxed);
    clients.join().unwrap();
}
