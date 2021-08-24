// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{
    io::{self, Read, Write},
    rc::{Rc, Weak},
    cell::{RefCell, RefMut},
    net::Shutdown,
};
use mio::{Token, Interest, net::TcpStream};

pub struct MarkedStream {
    stream: TcpStream,
    reader: bool,
    reader_discarded: bool,
    writer: bool,
    writer_discarded: bool,
}

impl AsMut<TcpStream> for MarkedStream {
    fn as_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}

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
                writer: false,
                writer_discarded: false,
            })),
            token,
        }
    }

    pub fn write_once(&self) -> Option<WriteOnce> {
        let mut s = self.inner.borrow_mut();
        if !s.writer && !s.writer_discarded {
            s.writer = true;
            Some(WriteOnce(Rc::downgrade(&self.inner)))
        } else {
            None
        }
    }

    pub fn read_once(&self) -> Option<ReadOnce> {
        let mut s = self.inner.borrow_mut();
        if !s.reader && !s.reader_discarded {
            s.reader = true;
            Some(ReadOnce(Rc::downgrade(&self.inner)))
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

#[must_use = "discard it if don't need"]
pub struct WriteOnce(Weak<RefCell<MarkedStream>>);

impl WriteOnce {
    pub fn write(self, data: &[u8]) -> io::Result<()> {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.as_mut().write_all(data)?;
            s.as_mut().flush()
        } else {
            Err(io::ErrorKind::NotConnected.into())
        }
    }

    pub fn write_last(self, data: &[u8]) -> io::Result<()> {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.as_mut().write_all(data)?;
            s.writer_discarded = true;
            s.as_mut().shutdown(Shutdown::Write)
        } else {
            Err(io::ErrorKind::NotConnected.into())
        }
    }

    pub fn discard(self) -> io::Result<()> {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.writer_discarded = true;
            s.as_mut().shutdown(Shutdown::Write)
        } else {
            Ok(())
        }
    }
}

impl Drop for WriteOnce {
    fn drop(&mut self) {
        if let Some(s) = self.0.upgrade() {
            s.borrow_mut().writer = false;
        }
    }
}

#[must_use = "discard it if don't need"]
pub struct ReadOnce(Weak<RefCell<MarkedStream>>);

impl ReadOnce {
    pub fn read(self, buf: &mut [u8]) -> io::Result<DidRead> {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.stream.read(buf).map(|length| DidRead {
                length,
                end: s.reader_discarded,
            })
        } else {
            Err(io::ErrorKind::NotConnected.into())
        }
    }

    pub fn discard(self) -> io::Result<()> {
        if let Some(s) = self.0.upgrade() {
            let mut s = s.borrow_mut();
            s.reader_discarded = true;
            s.as_mut().shutdown(Shutdown::Read)
        } else {
            Ok(())
        }
    }
}

impl Drop for ReadOnce {
    fn drop(&mut self) {
        if let Some(s) = self.0.upgrade() {
            s.borrow_mut().reader = false;
        }
    }
}

pub struct DidRead {
    pub length: usize,
    pub end: bool,
}
