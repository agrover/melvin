// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Communicating with `lvmetad`.

use unix_socket::UnixStream;

use std::io::{Result, Read, Write};
use std::io::Error;
use std::io::ErrorKind::Other;

use parser::{
    LvmTextMap,
    TextMapOps,
    buf_to_textmap,
    vg_from_textmap,
    textmap_to_buf,
};
use vg;


const LVMETAD_PATH: &'static str = "/run/lvm/lvmetad.socket";

fn collect_response(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut response = [0; 32];
    let mut v = Vec::new();

    loop {
        let bytes_read = try!(stream.read(&mut response));

        v.extend(&response[..bytes_read]);

        if v.ends_with(b"\n##\n") {
            // drop the end marker
            let len = v.len() - 4;
            v.truncate(len);
            return Ok(v);
        }
    }
}

fn _request(req: &[u8],
                token: Option<&[u8]>,
                stream: &mut UnixStream,
                args: &Option<Vec<&[u8]>>) -> Result<Vec<u8>> {

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

    try!(stream.write_all(&v));
    try!(stream.write_all(b"\n##\n"));

    collect_response(stream)
}

/// Make a request to the running lvmetad daemon.
///
/// # Examples
///
/// ```
///    use melvin::lvmetad::request;
///
///    let vg_list = request(b"vg_list", None);
/// ```
pub fn request(req: &[u8], args: Option<Vec<&[u8]>>) -> Result<LvmTextMap> {
    let err = || Error::new(Other, "response parsing error");
    let token = b"0";

    let mut stream = try!(UnixStream::connect(LVMETAD_PATH));

    let txt = try!(_request(req, Some(token), &mut stream, &args));
    let mut response = try!(buf_to_textmap(&txt));

    if try!(response.string_from_textmap("response").ok_or(err())) == "token_mismatch" {
        try!(_request(b"token_update", Some(token), &mut stream, &None));
        response = try!(_request(req, Some(token), &mut stream, &args)
            .and_then(|r| buf_to_textmap(&r)));
    }

    if response.get("global_invalid").is_some() || response.get("vg_invalid").is_some() {
        return Err(Error::new(Other, "cached metadata flagged as invalid"));
    }

    if try!(response.string_from_textmap("response").ok_or(err())) != "OK" {
        let reason = match response.string_from_textmap("reason") {
            Some(x) => x,
            None => "no reason given",
        };
        return Err(Error::new(Other, reason));
    }

    response.remove("response");

    Ok(response)
}

/// Query `lvmetad` for a list of Volume Groups on the system.
pub fn vg_list() -> Result<Vec<vg::VG>> {
    let err = || Error::new(Other, "response parsing error");
    let mut v = Vec::new();

    let vg_list = try!(request(b"vg_list", None));
    let vgs = try!(vg_list.textmap_from_textmap("volume_groups").ok_or(err()));

    for id in vgs.keys() {
        let name = try!(vgs.textmap_from_textmap(id)
                        .and_then(|val| val.string_from_textmap("name"))
                        .ok_or(err()));

        let mut option: Vec<u8> = Vec::new();
        option.extend(b"uuid = \"");
        option.extend(id.as_bytes());
        option.extend(b"\"");
        let options = vec!(&option[..]);

        let vg_info = try!(request(b"vg_lookup", Some(options)));
        let md = try!(vg_info.textmap_from_textmap("metadata").ok_or(err()));

        let vg = vg_from_textmap(&name, md).expect("didn't get vg!");

        v.push(vg);
    }

    Ok(v)
}

/// Tell `lvmetad` about the current state of a Volume Group.
pub fn vg_update(map: &LvmTextMap) -> Result<()> {

    assert_eq!(map.len(), 1);

    let k = map.keys().next().unwrap();
    let v = map.textmap_from_textmap(k).unwrap();

    let option = format!("vgname = \"{}\"", k);

    let mut option2 = Vec::new();
    option2.extend(b"metadata {");
    option2.extend(textmap_to_buf(v));
    option2.extend(b"}");

    let options = vec![option.as_bytes(), &option2];

    try!(request(b"vg_update", Some(options)));

    Ok(())
}
