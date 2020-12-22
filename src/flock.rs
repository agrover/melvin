use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use nix::fcntl::{flock, FlockArg};

use crate::Result;

const LVM_LOCK_DIR: &str = "/run/lock/lvm";

pub struct Flock {
    _locked_file: File,
}

pub enum LockScope {
    Global,
    VG(String),
}

impl Flock {
    pub fn lock_exclusive(scope: LockScope) -> Result<Flock> {
        Self::lock(scope, FlockArg::LockExclusive)
    }

    pub fn lock_shared(scope: LockScope) -> Result<Flock> {
        Self::lock(scope, FlockArg::LockShared)
    }

    fn lock(scope: LockScope, lock_type: FlockArg) -> Result<Flock> {
        let mut pathbuf: PathBuf = LVM_LOCK_DIR.into();
        let filename: Cow<Path> = match scope {
            LockScope::Global => Cow::Borrowed(Path::new("P_global")),
            LockScope::VG(name) => Cow::Owned(PathBuf::from(format!("V_{}", name))),
        };
        pathbuf.push(filename);

        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&pathbuf)?;
        flock(f.as_raw_fd(), lock_type)?;
        Ok(Flock { _locked_file: f })
    }

    // When the file is closed the lock is released.
}
