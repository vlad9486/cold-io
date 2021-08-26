// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{
    io::{self, Read, Write},
    rc::{Rc, Weak},
    cell::{RefCell, RefMut},
    net::Shutdown,
};
use mio::{Token, Interest, net::TcpStream};
use super::{
    marked_stream::MarkedStream,
    proposal::{ReadOnce, WriteOnce, IoResult},
};

pub struct ManagedStream {
    inner: Rc<RefCell<MarkedStream>>,
    token: Token,
}

impl ManagedStream {
    pub fn new(stream: TcpStream, token: Token) -> Self {
        ManagedStream {
            inner: Rc::new(RefCell::new(MarkedStream {
                stream,
                reader: false,
                reader_discarded: false,
                reader_used: false,
                writer: false,
                writer_discarded: false,
                writer_used: false,
            })),
            token,
        }
    }

    pub fn write_once(&self) -> Option<TcpWriteOnce> {
        let mut s = self.inner.borrow_mut();
        if !s.writer && !s.writer_discarded {
            s.writer = true;
            Some(TcpWriteOnce(Rc::downgrade(&self.inner)))
        } else {
            None
        }
    }

    pub fn read_once(&self) -> Option<TcpReadOnce> {
        let mut s = self.inner.borrow_mut();
        if !s.reader && !s.reader_discarded {
            s.reader = true;
            Some(TcpReadOnce(Rc::downgrade(&self.inner)))
        } else {
            None
        }
    }

    pub fn discard(self) -> io::Result<()> {
        let mut s = self.inner.borrow_mut();
        s.reader_discarded = true;
        s.writer_discarded = true;
        s.as_mut().shutdown(Shutdown::Both)
    }

    pub fn borrow_mut(&self) -> RefMut<MarkedStream> {
        self.inner.as_ref().borrow_mut()
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn closed(&self) -> bool {
        let s = self.inner.borrow();
        s.reader_discarded && s.writer_discarded
    }

    pub fn set_read_closed(&self) {
        self.borrow_mut().reader_discarded = true;
    }

    pub fn set_write_closed(&self) {
        self.borrow_mut().writer_discarded = true;
    }

    pub fn interests(&self) -> Option<Interest> {
        let s = self.inner.borrow();
        let read = !s.reader && !s.reader_discarded;
        let write = !s.writer && !s.writer_discarded;
        match (read, write) {
            (true, true) => Some(Interest::READABLE | Interest::WRITABLE),
            (true, false) => Some(Interest::READABLE),
            (false, true) => Some(Interest::WRITABLE),
            (false, false) => None,
        }
    }
}

pub struct TcpWriteOnce(Weak<RefCell<MarkedStream>>);

impl WriteOnce for TcpWriteOnce {
    fn write(self, data: &[u8]) -> IoResult {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            let will_close = s.writer_discarded;
            s.writer_used = true;
            match s.as_mut().write(data) {
                Ok(length) => IoResult::Done { length, will_close },
                Err(error) => {
                    log::error!("io error: {}", error);
                    match error.kind() {
                        io::ErrorKind::NotConnected => IoResult::Closed,
                        io::ErrorKind::WouldBlock => IoResult::Done {
                            length: 0,
                            will_close,
                        },
                        _ => IoResult::Closed,
                    }
                },
            }
        } else {
            IoResult::Closed
        }
    }
}

impl Drop for TcpWriteOnce {
    fn drop(&mut self) {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.writer_discarded = !s.writer_used;
            s.writer_used = false;
            s.writer = false;
            if let Err(error) = s.as_mut().shutdown(Shutdown::Write) {
                // it is expected the socket is not connected,
                // don't report this case
                if !matches!(error.kind(), io::ErrorKind::NotConnected) {
                    log::error!("io error: {}", error);
                }
            }
        }
    }
}

#[must_use = "discard it if don't need"]
pub struct TcpReadOnce(Weak<RefCell<MarkedStream>>);

impl ReadOnce for TcpReadOnce {
    fn read(self, buf: &mut [u8]) -> IoResult {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            let will_close = s.reader_discarded;
            s.reader_used = true;
            match s.as_mut().read(buf) {
                Ok(length) => IoResult::Done { length, will_close },
                Err(error) => {
                    log::error!("io error: {}", error);
                    match error.kind() {
                        io::ErrorKind::NotConnected => IoResult::Closed,
                        io::ErrorKind::WouldBlock => IoResult::Done {
                            length: 0,
                            will_close,
                        },
                        _ => IoResult::Closed,
                    }
                },
            }
        } else {
            IoResult::Closed
        }
    }
}

impl Drop for TcpReadOnce {
    fn drop(&mut self) {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.reader_discarded = !s.reader_used;
            s.reader_used = false;
            s.reader = false;
            if let Err(error) = s.as_mut().shutdown(Shutdown::Read) {
                // it is expected the socket is not connected,
                // don't report this case
                if !matches!(error.kind(), io::ErrorKind::NotConnected) {
                    log::error!("io error: {}", error);
                }
            }
        }
    }
}
