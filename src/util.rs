// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crc::crc32;

const INITIAL_CRC: u32 = 0xf597a6cf;
const CRC_SEED: u32 = 0xedb88320;

pub fn align_to(num: usize, align_to: usize) -> usize {
    let agn = align_to - 1;

    (num + agn) & !agn
}

pub fn crc32_calc(buf: &[u8]) -> u32 {
    let table = crc32::make_table(CRC_SEED);

    // For some reason, we need to negate the initial CRC value
    // and the result, to match what LVM2 is generating.
    !crc32::update(!INITIAL_CRC, &table, buf)
}
