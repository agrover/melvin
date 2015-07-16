use unix_socket::UnixStream;

use std::io::{Result, Read, Write};
use std::io::Error;
use std::io::ErrorKind::Other;

use parser::{LvmTextMap, TextMapOps, into_textmap};
use vg;
use parser;

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

pub fn _request(s: &[u8],
                token: Option<&[u8]>,
                stream: &mut UnixStream,
                args: Option<&[&[u8]]>) -> Result<Vec<u8>> {
    try!(stream.write_all(b"request = \""));
    try!(stream.write_all(s));
    try!(stream.write_all(b"\"\n"));

    if let Some(token) = token {
        try!(stream.write_all(b"token = \""));
        try!(stream.write_all(token));
        try!(stream.write_all(b"\"\n"));
        try!(stream.write_all(b"\n"));
    }

    if let Some(args) = args {
        for arg in args {
            try!(stream.write_all(arg));
            try!(stream.write_all(b"\n"));
        }
    }

    try!(stream.write_all(b"\n##\n"));

    collect_response(stream)
}

pub fn request(s: &[u8], args: Option<&[&[u8]]>) -> Result<LvmTextMap> {
    let err = || Error::new(Other, "response parsing error");
    let token = b"0";

    let mut stream = try!(UnixStream::connect(LVMETAD_PATH));

    let txt = try!(_request(s, Some(token), &mut stream, args));
    let mut response = try!(into_textmap(&txt));

    if try!(response.string_from_textmap("response").ok_or(err())) == "token_mismatch" {
        try!(_request(b"token_update", Some(token), &mut stream, None));
        response = try!(_request(s, Some(token), &mut stream, args)
            .and_then(|r| into_textmap(&r)));
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

pub fn vgs_from_lvmetad() -> Result<Vec<vg::VG>> {
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

        let vg_info = try!(request(b"vg_lookup", Some(&options[..])));
        let md = try!(vg_info.textmap_from_textmap("metadata").ok_or(err()));

        let vg = parser::vg_from_textmap(&name, md).expect("didn't get vg!");

        v.push(vg);
    }

    Ok(v)
}

pub fn vg_update_lvmetad(map: &LvmTextMap) -> Result<()> {

    assert_eq!(map.len(), 1);

    let k = map.keys().next().unwrap();
    let v = map.textmap_from_textmap(k).unwrap();

    let mut option = Vec::new();
    option.extend(b"vgname = \"");
    option.extend(k.as_bytes());
    option.extend(b"\"");

    let mut option2 = Vec::new();
    option2.extend(b"metadata {");
    option2.extend(textmap_to_buf(v));
    option2.extend(b"}");

    let mut options: Vec<&[u8]> = Vec::new();

    options.push(&option);
    options.push(&option2);

    try!(request(b"vg_update", Some(options)));

    Ok(())
}
