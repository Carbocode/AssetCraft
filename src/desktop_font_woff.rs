//! Unpack web fonts into an SFNT for metadata, thumbnails and live specimens.
//! https://www.w3.org/TR/WOFF/

use flate2::bufread::ZlibDecoder;
use std::io::Read;

pub(crate) const MAX_FONT_BYTES: usize = 64 << 20;

pub(crate) fn decode(data: &[u8]) -> Option<Vec<u8>> {
    if data.starts_with(b"wOF2") {
        let header = data.get(..48)?;
        let count = u16::from_be_bytes(header[12..14].try_into().ok()?);
        let size = number(&header[16..20])?;
        if number(&header[8..12])? != data.len()
            || !(1..4096).contains(&count)
            || !(12..=MAX_FONT_BYTES).contains(&size)
            || woff2_sizes(&data[48..], count).is_none()
        {
            return None;
        }
        let font = wuff::decompress_woff2(data).ok()?;
        return (font.len() <= MAX_FONT_BYTES).then_some(font);
    }
    let header = data.get(..44)?;
    let flavor = &header[4..8];
    if &header[..4] != b"wOFF"
        || !matches!(flavor, b"\0\x01\0\0" | b"OTTO" | b"true")
        || number(&header[8..12])? != data.len()
        || header[14..16] != [0, 0]
    {
        return None;
    }
    let count = u16::from_be_bytes(header[12..14].try_into().ok()?);
    if !(1..4096).contains(&count) {
        return None;
    }
    let sfnt_len = number(&header[16..20])?;
    let directory_end = 12 + usize::from(count) * 16;
    if !(directory_end..=MAX_FONT_BYTES).contains(&sfnt_len) {
        return None;
    }
    let woff_directory_end = 44 + usize::from(count) * 20;
    let mut output = vec![0; directory_end];
    output[..4].copy_from_slice(flavor);
    output[4..6].copy_from_slice(&count.to_be_bytes());
    let exponent = count.ilog2() as u16;
    let search_range = (1 << exponent) * 16;
    output[6..8].copy_from_slice(&u16::to_be_bytes(search_range));
    output[8..10].copy_from_slice(&exponent.to_be_bytes());
    output[10..12].copy_from_slice(&(count * 16 - search_range).to_be_bytes());
    let mut prior_tag = None;
    let mut ranges = Vec::with_capacity(usize::from(count));
    for (index, record) in data
        .get(44..woff_directory_end)?
        .as_chunks::<20>()
        .0
        .iter()
        .enumerate()
    {
        let tag = &record[..4];
        let offset = number(&record[4..8])?;
        let compressed_len = number(&record[8..12])?;
        let original_len = number(&record[12..16])?;
        let start = output.len();
        let padded_end = start.checked_add(original_len)?.next_multiple_of(4);
        if prior_tag.is_some_and(|prior| prior >= tag)
            || offset < woff_directory_end
            || offset % 4 != 0
            || compressed_len > original_len
            || padded_end > sfnt_len
        {
            return None;
        }
        prior_tag = Some(tag);

        let end = offset.checked_add(compressed_len)?;
        if end > offset {
            ranges.push((offset, end));
        }
        let compressed = data.get(offset..end)?;
        if compressed_len == original_len {
            output.extend_from_slice(compressed);
        } else {
            let mut decoder = ZlibDecoder::new(compressed);
            (&mut decoder)
                .take(original_len as u64 + 1)
                .read_to_end(&mut output)
                .ok()?;
            if output.len() != start + original_len || decoder.total_in() != compressed_len as u64 {
                return None;
            }
        }
        output.resize(padded_end, 0);
        let table = &mut output[12 + index * 16..28 + index * 16];
        table[..4].copy_from_slice(tag);
        table[4..8].copy_from_slice(&record[16..20]);
        table[8..12].copy_from_slice(&(start as u32).to_be_bytes());
        table[12..16].copy_from_slice(&(original_len as u32).to_be_bytes());
    }
    ranges.sort_unstable();
    let overlap = ranges.windows(2).any(|pair| pair[0].1 > pair[1].0);
    (output.len() == sfnt_len && !overlap).then_some(output)
}

fn woff2_sizes(mut directory: &[u8], count: u16) -> Option<()> {
    let mut original_left = MAX_FONT_BYTES.checked_sub(12 + usize::from(count) * 16)?;
    let mut transformed_left = MAX_FONT_BYTES;
    for _ in 0..count {
        let flags = *directory.split_off_first()?;
        let index = flags & 0x3f;
        let glyf_or_loca = match index {
            10 | 11 => true,
            63 => matches!(directory.split_off(..4)?, b"glyf" | b"loca"),
            _ => false,
        };
        let transform = flags >> 6;
        let original = base128(&mut directory)?;
        let stored = if if glyf_or_loca {
            transform != 3
        } else {
            transform != 0
        } {
            base128(&mut directory)?
        } else {
            original
        };
        original_left = original_left.checked_sub(original.checked_next_multiple_of(4)?)?;
        transformed_left = transformed_left.checked_sub(stored)?;
    }
    Some(())
}

fn base128(bytes: &mut &[u8]) -> Option<usize> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = *bytes.split_off_first()?;
        if (index == 0 && byte == 0x80) || value > (u32::MAX >> 7) {
            return None;
        }
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte < 0x80 {
            return usize::try_from(value).ok();
        }
    }
    None
}

fn number(bytes: &[u8]) -> Option<usize> {
    usize::try_from(u32::from_be_bytes(bytes.try_into().ok()?)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    #[test]
    fn woff_tables_rebuild_a_bounded_sfnt_directory() {
        let bytes = [b'A'; 97];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut woff = vec![0; 100];
        woff[..8].copy_from_slice(b"wOFFOTTO");
        woff[12..14].copy_from_slice(&2u16.to_be_bytes());
        woff[16..20].copy_from_slice(&148u32.to_be_bytes());
        woff[44..48].copy_from_slice(b"CFF ");
        woff[48..52].copy_from_slice(&84u32.to_be_bytes());
        woff[52..56].copy_from_slice(&4u32.to_be_bytes());
        woff[56..60].copy_from_slice(&4u32.to_be_bytes());
        woff[64..68].copy_from_slice(b"name");
        woff[68..72].copy_from_slice(&88u32.to_be_bytes());
        woff[72..76].copy_from_slice(&(compressed.len() as u32).to_be_bytes());
        woff[76..80].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
        woff[84..88].copy_from_slice(b"data");
        woff.truncate(88);
        woff.extend_from_slice(&compressed);
        woff.resize(woff.len().next_multiple_of(4), 0);
        let file_len = woff.len() as u32;
        woff[8..12].copy_from_slice(&file_len.to_be_bytes());

        let sfnt = decode(&woff).unwrap();
        assert_eq!(&sfnt[..12], b"OTTO\0\x02\0\x20\0\x01\0\0");
        assert_eq!(&sfnt[12..16], b"CFF ");
        assert_eq!(&sfnt[20..24], &44u32.to_be_bytes());
        assert_eq!(&sfnt[36..40], &48u32.to_be_bytes());
        assert_eq!(&sfnt[44..48], b"data");
        assert_eq!(&sfnt[48..145], &bytes);
        assert_eq!(&sfnt[145..], &[0, 0, 0]);

        woff[76..80].copy_from_slice(&96u32.to_be_bytes());
        assert!(decode(&woff).is_none());
        woff[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&woff).is_none());
    }
}
