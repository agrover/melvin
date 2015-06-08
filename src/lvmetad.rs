use unix_socket::UnixStream;

use std::io;
use std::io::{Read, Write};

const LVMETAD_PATH: &'static str = "/run/lvm/lvmetad.socket";

fn response(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut response = [0; 32];
    let mut v = Vec::new();

    loop {
        let bytes_read = try!(stream.read(&mut response));

        v.push_all(&response[..bytes_read]);

        if v.ends_with(b"\n##\n") {
            // drop the end marker
            let len = v.len() - 4;
            v.truncate(len);
            return Ok(v);
        }
    }
}

fn open_lvmetad() {

    request(b"hello", false);
//    lvmetad_request(b"vg_list", true);
//    lvmetad_request(b"pv_list", true);
//    lvmetad_request(b"dump", false);

}

fn request(s: &[u8], token: bool) -> io::Result<Vec<u8>>{

    let mut stream = UnixStream::connect(LVMETAD_PATH).unwrap();
    stream.write_all(b"request = \"").unwrap();
    stream.write_all(s).unwrap();
    stream.write_all(b"\"\n").unwrap();
    if token {
        stream.write_all(b"token = \"filter:0\"").unwrap();
        stream.write_all(b"\n").unwrap();
    }
    stream.write_all(b"\n##\n").unwrap();

    let response = try!(response(&mut stream));

    Ok(response)
}
