const NAME: &[u8] = b"log.txt";

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

fn crc32(data: &[u8]) -> u32 {
    !data
        .iter()
        .fold(!0u32, |c, &b| TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8))
}

fn header(out: &mut Vec<u8>, signature: u32, crc: u32, packed: u32, size: u32, offset: Option<u32>) {
    let central = offset.is_some();
    out.extend(signature.to_le_bytes());
    if central {
        out.extend(20u16.to_le_bytes());
    }

    for field in [20u16, 0, 8, 0, 0x21] {
        out.extend(field.to_le_bytes());
    }
    for field in [crc, packed, size] {
        out.extend(field.to_le_bytes());
    }
    out.extend((NAME.len() as u16).to_le_bytes());
    out.extend(0u16.to_le_bytes());

    if let Some(offset) = offset {
        out.extend([0u8; 6]);
        out.extend(0u32.to_le_bytes());
        out.extend(offset.to_le_bytes());
    }
    out.extend(NAME);
}

pub fn pack(text: &str) -> Vec<u8> {
    let data = text.as_bytes();
    let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
    let (crc, size, len) = (crc32(data), data.len() as u32, packed.len() as u32);

    let mut out = Vec::with_capacity(packed.len() + 128);
    header(&mut out, 0x0403_4B50, crc, len, size, None);
    out.extend(&packed);
    drop(packed);

    let central = out.len() as u32;
    header(&mut out, 0x0201_4B50, crc, len, size, Some(0));
    let directory = out.len() as u32 - central;

    out.extend(0x0605_4B50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(directory.to_le_bytes());
    out.extend(central.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    #[cfg(unix)]
    fn unzip() {
        let text = "1|2\n3\nhello hello hello\n";
        let file = std::env::temp_dir().join("warlogger-test.zip");
        std::fs::write(&file, super::pack(text)).unwrap();

        let out = std::process::Command::new("unzip")
            .arg("-p")
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), text);
    }
}
