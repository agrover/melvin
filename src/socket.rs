// Copyright © 2015 Geoffroy Couprie
// Copyright © 2015 Andy Grover
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// “Software”), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

// This code is based on https://github.com/Geal/rust-syslog.

extern crate libc;
extern crate errno;

use std::intrinsics;
use std::mem;
use std::ffi::CString;
use std::sync::Arc;
use std::os;
use std::sync::Mutex;
use std::os::unix::io::RawFd;
use std::io::{Error, ErrorKind};
use std::io::Result;
use std::slice::from_raw_parts;
use errno::{Errno, errno, set_errno};

struct Inner {
    fd: RawFd
}

impl Inner {
    fn new(fd: RawFd) -> Inner {
        Inner { fd: fd }
    }
}

impl Drop for Inner {
    fn drop(&mut self) { unsafe { let _ = libc::close(self.fd); } }
}

//fn sockaddr_to_unix(storage: &libc::sockaddr_storage,
//                    len: usize) -> Result<CString> {
//    match storage.ss_family as libc::c_int {
//        libc::AF_UNIX => {
//            assert!(len as usize <= mem::size_of::<libc::sockaddr_un>());
//            let storage: &libc::sockaddr_un = unsafe {
//                mem::transmute(storage)
//            };
//            unsafe {
//                //FIXME: the array size depends on the platform
//                let tmp = from_raw_parts(mem::transmute(&storage.sun_path), 104);
//                Ok(CString::from_vec_unchecked(tmp))
//            }
//        }
//        _ => Err(Error::new(ErrorKind::InvalidInput, "dunno"))
//    }
//}

#[inline]
fn retry<F>(mut f: F) -> libc::c_int where F: FnMut() -> libc::c_int {
    loop {
        match f() {
            -1 if errno() == libc::EINTR => {}
            n => return n,
        }
    }
}

fn last_error() -> Error {
    Error::last_os_error()
}

fn addr_to_sockaddr_un(addr: &CString) -> Result<(libc::sockaddr_storage, usize)> {
    // the sun_path length is limited to SUN_LEN (with null)
    assert!(mem::size_of::<libc::sockaddr_storage>() >=
            mem::size_of::<libc::sockaddr_un>());
    let mut storage: libc::sockaddr_storage = unsafe { intrinsics::init() };
    let s: &mut libc::sockaddr_un = unsafe { mem::transmute(&mut storage) };

    let len = addr.as_bytes().len();
    if len > s.sun_path.len() - 1 {
        return Err(Error::new(ErrorKind::InvalidInput,
                              "path must be smaller than SUN_LEN"));
    }
    s.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (slot, value) in s.sun_path.iter_mut().zip(addr.as_bytes().iter()) {
        *slot = *value as i8;
    }

    // count the null terminator
    let len = mem::size_of::<libc::sa_family_t>() + len + 1;
    return Ok((storage, len));
}

fn unix_socket(ty: libc::c_int) -> Result<RawFd> {
    match unsafe { libc::socket(libc::AF_UNIX, ty, 0) } {
        -1 => Err(last_error()),
        fd => Ok(fd)
    }
}

fn connect(addr: &CString, ty: libc::c_int) -> Result<Inner> {
    let (addr, len) = try!(addr_to_sockaddr_un(addr));
    let inner = Inner { fd: try!(unix_socket(ty))};
    let addrp = &addr as *const libc::sockaddr_storage;
    match retry(|| unsafe {
        libc::connect(inner.fd, addrp as *const libc::sockaddr,
                      len as libc::socklen_t)
    }) {
        -1 => Err(last_error()),
        _  => Ok(inner)
    }
}

fn bind(addr: &CString, ty: libc::c_int) -> Result<Inner> {
    let (addr, len) = try!(addr_to_sockaddr_un(addr));
    let inner = Inner::new(try!(unix_socket(ty)));
    let addrp = &addr as *const libc::sockaddr_storage;
    match unsafe {
        libc::bind(inner.fd, addrp as *const libc::sockaddr, len as libc::socklen_t)
    } {
        -1 => Err(last_error()),
        _  => Ok(inner)
    }
}

////////////////////////////////////////////////////////////////////////////////
// Unix Datagram
////////////////////////////////////////////////////////////////////////////////

pub struct UnixDatagram {
    inner: Arc<Inner>,
}

impl UnixDatagram {
    pub fn connect(addr: &CString) -> Result<UnixDatagram> {
        connect(addr, libc::SOCK_DGRAM).map(|inner| {
            UnixDatagram { inner: Arc::new(inner) }
        })
    }
    pub fn bind(addr: &CString) -> Result<UnixDatagram> {
        bind(addr, libc::SOCK_DGRAM).map(|inner| {
            UnixDatagram { inner: Arc::new(inner) }
        })
    }

    fn fd(&self) -> RawFd { (*self.inner).fd }

    pub fn recvfrom(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut storage: libc::sockaddr_storage = unsafe { intrinsics::init() };
        let storagep = &mut storage as *mut libc::sockaddr_storage;
        let mut addrlen: libc::socklen_t =
            mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        let ret = retry(|| unsafe {
            libc::recvfrom(self.fd(),
                           buf.as_ptr() as *mut libc::c_void,
                           buf.len() as libc::size_t,
                           0,
                           storagep as *mut libc::sockaddr,
                           &mut addrlen) as libc::c_int
        });

        if ret < 0 { return Err(last_error()) }

        Ok(ret as usize)
    }


    pub fn sendto(&mut self, buf: &[u8], dst: &CString) -> Result<()> {
        let (dst, len) = try!(addr_to_sockaddr_un(dst));
        let dstp = &dst as *const libc::sockaddr_storage;
        let ret = retry(|| unsafe {
            libc::sendto(self.fd(),
                         buf.as_ptr() as *const libc::c_void,
                         buf.len() as libc::size_t,
                         0,
                         dstp as *const libc::sockaddr,
                         len as libc::socklen_t) as libc::c_int
        });

        match ret {
            -1 => Err(last_error()),
            n if n as usize != buf.len() => {
                Err(Error::new(ErrorKind::InvalidInput,
                               "couldn't send entire packet at once"))
            }
            _ => Ok(())
        }
    }

    pub fn clone(&mut self) -> UnixDatagram {
        UnixDatagram { inner: self.inner.clone() }
    }
}
