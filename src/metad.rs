// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Communicating with `lvmetad`.

use unix_socket::UnixStream;

use std::io;
use std::io::ErrorKind::Other;
use std::io::{Read, Write};

use crate::parser::{buf_to_textmap, textmap_to_buf, LvmTextMap, TextMapOps};
use crate::vg;
use crate::{Error, Result};
use vg::VG;

const LVMETAD_PATH: &'static str = "/run/lvm/lvmetad.socket";

fn collect_response(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut response = [0; 32];
    let mut v = Vec::new();

    loop {
        let bytes_read = stream.read(&mut response)?;

        v.extend(&response[..bytes_read]);

        if v.ends_with(b"\n##\n") {
            // drop the end marker
            let len = v.len() - 4;
            v.truncate(len);
            return Ok(v);
        }
    }
}

fn _request(
    req: &[u8],
    token: Option<&[u8]>,
    stream: &mut UnixStream,
    args: &Option<Vec<&[u8]>>,
) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    v.extend(b"request = \"");
    v.extend(req);
    v.extend(b"\"\n");

    if let Some(token) = token {
        v.extend(b"token = \"filter:");
        v.extend(token);
        v.extend(b"\"\n");
    }

    if let &Some(ref args) = args {
        for arg in args {
            v.extend(arg.clone());
            v.extend(b"\n");
        }
    }

    stream.write_all(&v)?;
    stream.write_all(b"\n##\n")?;

    collect_response(stream)
}

/// Make a request to the running lvmetad daemon.
pub fn request(req: &[u8], args: Option<Vec<&[u8]>>) -> Result<LvmTextMap> {
    let err = || Error::Io(io::Error::new(Other, "response parsing error"));
    let token = b"0";

    let mut stream = UnixStream::connect(LVMETAD_PATH)?;

    let txt = _request(req, Some(token), &mut stream, &args)?;
    let mut response = buf_to_textmap(&txt)?;

    if response.string_from_textmap("response").ok_or(err())? == "token_mismatch" {
        _request(b"token_update", Some(token), &mut stream, &None)?;
        response =
            _request(req, Some(token), &mut stream, &args).and_then(|r| buf_to_textmap(&r))?;
    }

    if response.get("global_invalid").is_some() || response.get("vg_invalid").is_some() {
        return Err(Error::Io(io::Error::new(
            Other,
            "cached metadata flagged as invalid",
        )));
    }

    if response.string_from_textmap("response").ok_or(err())? != "OK" {
        let reason = match response.string_from_textmap("reason") {
            Some(x) => x,
            None => "no reason given",
        };
        return Err(Error::Io(io::Error::new(Other, reason)));
    }

    response.remove("response");

    Ok(response)
}

/// Query `lvmetad` for a list of Volume Groups on the system.
///
/// # Examples
///
/// ```
///    use melvin::vg_list;
///
///    let vgs = vg_list();
/// ```
pub fn vg_list() -> Result<Vec<VG>> {
    let err = || Error::Io(io::Error::new(Other, "response parsing error"));
    let mut v = Vec::new();

    let vg_list = request(b"vg_list", None)?;
    let vgs = vg_list.textmap_from_textmap("volume_groups").ok_or(err())?;

    for id in vgs.keys() {
        let name = vgs
            .textmap_from_textmap(id)
            .and_then(|val| val.string_from_textmap("name"))
            .ok_or(err())?;

        let mut option: Vec<u8> = Vec::new();
        option.extend(b"uuid = \"");
        option.extend(id.as_bytes());
        option.extend(b"\"");
        let options = vec![&option[..]];

        let vg_info = request(b"vg_lookup", Some(options))?;
        let md = vg_info.textmap_from_textmap("metadata").ok_or(err())?;

        let vg = vg::from_textmap(&name, md).expect("didn't get vg!");

        v.push(vg);
    }

    Ok(v)
}

/// Tell `lvmetad` about the current state of a Volume Group. The
/// `map` is the layout generated by vg.into() for LvmTextMap.
pub fn vg_update(name: &str, map: &LvmTextMap) -> Result<()> {
    let option = format!("vgname = \"{}\"", name);

    let mut option2 = Vec::new();
    option2.extend(b"metadata {");
    option2.extend(textmap_to_buf(map));
    option2.extend(b"}");

    let options = vec![option.as_bytes(), &option2];

    request(b"vg_update", Some(options))?;

    Ok(())
}

/// Tell `lvmetad` about a new PV. The `map` is that generated by
/// converting a `PvHeader` into an `LvmTextMap`.
pub fn pv_found(map: &LvmTextMap) -> Result<()> {
    let mut option = Vec::new();
    option.extend(b"pvmeta {");
    option.extend(textmap_to_buf(map));
    option.extend(b"}");

    let options = vec![&option[..]];

    request(b"pv_found", Some(options))?;

    Ok(())
}
