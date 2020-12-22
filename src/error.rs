// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Dm(devicemapper::DmError),
    Nix(nix::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<devicemapper::DmError> for Error {
    fn from(err: devicemapper::DmError) -> Error {
        Error::Dm(err)
    }
}

impl From<nix::Error> for Error {
    fn from(err: nix::Error) -> Error {
        Error::Nix(err)
    }
}
