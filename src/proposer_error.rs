// Copyright 2021 Vladislav Melnik
// SPDX-License-Identifier: MIT

use std::{net::SocketAddr, io, fmt, error::Error};
use smallvec::SmallVec;

use super::request::ConnectionSource;

#[derive(Debug, Default)]
pub struct ProposerError {
    pub listen_error: Option<(ConnectionSource, io::Error)>,
    pub connect_errors: SmallVec<[(SocketAddr, io::Error); 8]>,
    pub disconnect_errors: SmallVec<[(SocketAddr, io::Error); 4]>,
    pub accept_error: Option<io::Error>,
    pub poll_error: Option<io::Error>,
}

impl fmt::Display for ProposerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((source, error)) = &self.listen_error {
            write!(f, "failed to listen: {}, error: {}", source, error)?;
        }
        for (addr, error) in &self.connect_errors {
            write!(f, "failed to connect to: {}, error: {}", addr, error)?;
        }
        for (addr, error) in &self.disconnect_errors {
            write!(f, "failed to disconnect from: {}, error: {}", addr, error)?;
        }
        if let Some(error) = &self.accept_error {
            write!(f, "failed to accept a connection, error: {}", error)?;
        }
        if let Some(error) = &self.poll_error {
            write!(f, "failed to poll the events, error: {}", error)?;
        }

        Ok(())
    }
}

impl Error for ProposerError {}

impl ProposerError {
    pub(super) fn take_result(&mut self) -> Result<(), Self> {
        use std::mem;

        if self.is_empty() {
            Ok(())
        } else {
            Err(ProposerError {
                listen_error: self.listen_error.take(),
                connect_errors: mem::take(&mut self.connect_errors),
                disconnect_errors: mem::take(&mut self.disconnect_errors),
                accept_error: self.accept_error.take(),
                poll_error: self.poll_error.take(),
            })
        }
    }

    fn is_empty(&self) -> bool {
        self.listen_error.is_none()
            && self.connect_errors.is_empty()
            && self.disconnect_errors.is_empty()
            && self.accept_error.is_none()
            && self.poll_error.is_none()
    }
}
