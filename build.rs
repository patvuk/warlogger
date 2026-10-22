fn entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(32u32.to_le_bytes());
    for v in [0xFFFF, kind, 0xFFFF, name] {
        out.extend(v.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.extend(0x0409u16.to_le_bytes());
    out.extend([0; 8]);
    out.extend(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

fn icon_res(ico: &[u8]) -> Vec<u8> {
    let le16 = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let le32 = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap()) as usize;

    let mut res = Vec::new();
    entry(&mut res, 0, 0, 0, &[]);

    let count = le16(4);
    let mut group = [0u16, 1, count]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    for i in 0..count as usize {
        let at = 6 + 16 * i;
        let (size, offset) = (le32(at + 8), le32(at + 12));
        entry(&mut res, 3, i as u16 + 1, 0x1010, &ico[offset..offset + size]);

        group.extend(&ico[at..at + 12]);
        group.extend((i as u16 + 1).to_le_bytes());
    }

    entry(&mut res, 14, 1, 0x1030, &group);
    res
}

fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    println!("cargo:rerun-if-changed=assets/warlogger.ico");

    if windows && msvc {
        let res = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("warlogger.res");
        std::fs::write(&res, icon_res(&std::fs::read("assets/warlogger.ico").unwrap())).unwrap();
        println!("cargo:rustc-link-arg-bins={}", res.display());
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
