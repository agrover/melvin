[![Build Status](https://travis-ci.org/agrover/melvin.svg?branch=master)](https://travis-ci.org/agrover/melvin)

## MeLVin

#### A 100% Rust implementation of [LVM](https://www.sourceware.org/lvm2/) and [libdevmapper](https://www.sourceware.org/dm/)

Currently a big pile of pieces, someday this could be a nice library
that provides a nice API to LVM that clients could use.

* parsing of metadata and lvm.conf
* locking
* dm ioctls
* lvmetad support (lvmetad required)
* no legacy support

### Development status

#### ALPHA. Do not test on a system with data you care about, especially any APIs that write anything (i.e take `&mut self` as an argument).

### Example

```rust
  use melvin::lvmetad;
  
  let mut vgs = lvmetad::vgs_from_lvmetad().expect("could not get vgs from lvmetad"); 
  let mut vg = vgs.pop().expect("no vgs in vgs");
  
  println!("first vg name = {} uuid = {}", vg.name, vg.id);
```

### How to contribute

GitHub used for pull requests and issue tracking

### License

[Mozilla Public License 2.0](https://www.mozilla.org/MPL/2.0/FAQ.html)
