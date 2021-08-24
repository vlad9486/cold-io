// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use mio::net::TcpStream;

pub struct MarkedStream {
    pub stream: TcpStream,
    pub reader: bool,
    pub reader_discarded: bool,
    pub writer: bool,
    pub writer_discarded: bool,
}

impl AsMut<TcpStream> for MarkedStream {
    fn as_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}
