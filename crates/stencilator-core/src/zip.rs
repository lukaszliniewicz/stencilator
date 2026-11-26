//! Zip archives that come out the same, byte for byte, every time.
//!
//! Entries are deflated, or stored when deflating would not make them smaller, and carry
//! a fixed date (1 January 1980, the earliest a zip can record) instead of the clock's.
//! Enough for a set of stencils and for 3MF, which is a zip with a fixed layout. Sizes
//! stop at 4 GB, the limit without the Zip64 extension.

use crate::Error;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
/// Version 2.0: deflate, folders.
const VERSION: u16 = 20;
/// The file name is UTF-8.
const FLAG_UTF8: u16 = 1 << 11;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// 1 January 1980 in MS-DOS date format; the time is midnight, 0.
const DOS_DATE: u16 = (1 << 5) | 1;
const DEFLATE_LEVEL: u8 = 6;

const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut n = 0;
    while n < 256 {
        #[allow(clippy::cast_possible_truncation, reason = "n < 256")]
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if (c & 1) == 1 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

/// The CRC-32 (IEEE) of some bytes, as zip records it.
fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0, |c, &b| CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8))
}

/// A zip archive holding each `(name, bytes)` pair, in the order given. Names use `/`
/// between folders.
///
/// # Errors
///
/// [`Error::Export`] when there are more than 65,535 entries, a name is empty or longer
/// than 65,535 bytes, or the archive would reach 4 GB.
pub fn zip<N: AsRef<str>, D: AsRef<[u8]>>(entries: &[(N, D)]) -> Result<Vec<u8>, Error> {
    let too_large = || Error::Export("The zip would be larger than 4 GB; export fewer or smaller files.".to_owned());
    let count =
        u16::try_from(entries.len()).map_err(|_| Error::Export("A zip holds at most 65,535 files.".to_owned()))?;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let (name, data) = (name.as_ref(), data.as_ref());
        let name_len = u16::try_from(name.len())
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| Error::Export(format!("{name:?} cannot be a file name in a zip.")))?;
        let deflated = miniz_oxide::deflate::compress_to_vec(data, DEFLATE_LEVEL);
        let (method, body) = if deflated.len() < data.len() { (DEFLATED, deflated.as_slice()) } else { (STORED, data) };
        let flags = if name.is_ascii() { 0 } else { FLAG_UTF8 };
        let crc = crc32(data);
        let size = u32::try_from(data.len()).map_err(|_| too_large())?;
        let packed = u32::try_from(body.len()).map_err(|_| too_large())?;
        let offset = u32::try_from(out.len()).map_err(|_| too_large())?;

        // Fields shared by the local and the central header, from "version needed" on.
        let mut common = Vec::with_capacity(26);
        for half in [VERSION, flags, method, 0, DOS_DATE] {
            common.extend_from_slice(&half.to_le_bytes());
        }
        for word in [crc, packed, size] {
            common.extend_from_slice(&word.to_le_bytes());
        }
        common.extend_from_slice(&name_len.to_le_bytes());
        common.extend_from_slice(&0_u16.to_le_bytes()); // no extra field

        out.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);

        central.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
        central.extend_from_slice(&VERSION.to_le_bytes()); // made by: MS-DOS, version 2.0
        central.extend_from_slice(&common);
        // Comment length, disk number, internal and external attributes.
        central.extend_from_slice(&[0; 10]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = u32::try_from(out.len()).map_err(|_| too_large())?;
    let central_size = u32::try_from(central.len()).map_err(|_| too_large())?;
    out.extend_from_slice(&central);
    u32::try_from(out.len() + 22).map_err(|_| too_large())?;
    out.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
    out.extend_from_slice(&[0; 4]); // this disk, and the disk where the directory starts
    for half in [count, count] {
        out.extend_from_slice(&half.to_le_bytes());
    }
    for word in [central_size, central_offset] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&0_u16.to_le_bytes()); // no comment
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// Reads an archive back through its central directory, as unzip tools do.
    fn unzip(archive: &[u8]) -> Vec<(String, Vec<u8>)> {
        let end = archive.len() - 22;
        assert_eq!(u32_at(archive, end), END_OF_CENTRAL_DIRECTORY);
        let count = u16_at(archive, end + 10);
        let mut at = u32_at(archive, end + 16) as usize;
        (0..count)
            .map(|_| {
                assert_eq!(u32_at(archive, at), CENTRAL_HEADER);
                let (method, crc) = (u16_at(archive, at + 10), u32_at(archive, at + 16));
                let (packed, size) = (u32_at(archive, at + 20) as usize, u32_at(archive, at + 24) as usize);
                let name_len = u16_at(archive, at + 28) as usize;
                let name = String::from_utf8(archive[at + 46..at + 46 + name_len].to_vec()).unwrap();
                let local = u32_at(archive, at + 42) as usize;
                assert_eq!(u32_at(archive, local), LOCAL_HEADER);
                assert_eq!(archive[local + 4..local + 30], archive[at + 6..at + 32], "{name}: headers differ");
                let start = local + 30 + name_len;
                let body = &archive[start..start + packed];
                let data = match method {
                    DEFLATED => miniz_oxide::inflate::decompress_to_vec(body).unwrap(),
                    STORED => body.to_vec(),
                    other => panic!("method {other}"),
                };
                assert_eq!((data.len(), crc32(&data)), (size, crc), "{name}");
                at += 46 + name_len;
                (name, data)
            })
            .collect()
    }

    #[test]
    fn crc_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn entries_come_back_unchanged_and_in_order() {
        let stl = vec![7_u8; 5000];
        let entries = [("plates/a.stl", stl.as_slice()), ("ż.txt", b"x".as_slice()), ("empty", b"".as_slice())];
        let archive = zip(&entries).unwrap();
        let back = unzip(&archive);
        assert_eq!(back.len(), 3);
        for ((name, data), (back_name, back_data)) in entries.iter().zip(&back) {
            assert_eq!((*name, *data), (back_name.as_str(), back_data.as_slice()));
        }
        // Repetitive data is deflated, a single byte is stored.
        assert!(archive.len() < 1000);
    }

    #[test]
    fn the_same_files_give_the_same_bytes() {
        let entries = [("a", vec![1_u8, 2, 3])];
        assert_eq!(zip(&entries).unwrap(), zip(&entries).unwrap());
        assert_eq!(zip::<&str, &[u8]>(&[]).unwrap().len(), 22);
    }

    #[test]
    fn nameless_files_are_refused() {
        assert!(zip(&[("", b"x")]).is_err());
    }
}
