use super::Response;
use crate::win::wide;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use windows_sys::Win32::Networking::WinHttp::*;

struct Handle(*mut core::ffi::c_void);

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn check(ok: i32, what: &str) -> Result<(), String> {
    match ok {
        0 => Err(format!("{what} failed: {}", std::io::Error::last_os_error())),
        _ => Ok(()),
    }
}

fn session() -> Result<&'static Handle, String> {
    static SESSION: OnceLock<Option<Handle>> = OnceLock::new();

    SESSION
        .get_or_init(|| unsafe {
            let agent = wide("warlogger");
            let h = WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0);
            if h.is_null() {
                return None;
            }

            let decompress = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
            WinHttpSetOption(h, WINHTTP_OPTION_DECOMPRESSION, (&raw const decompress).cast(), 4);
            WinHttpSetTimeouts(h, 30_000, 30_000, 300_000, 300_000);
            Some(Handle(h))
        })
        .as_ref()
        .ok_or_else(|| format!("WinHttpOpen failed: {}", std::io::Error::last_os_error()))
}

fn split(url: &str) -> Result<(&str, &str), String> {
    let rest = url.strip_prefix("https://").ok_or("only https urls are supported")?;
    Ok(rest.find('/').map_or((rest, "/"), |i| (&rest[..i], &rest[i..])))
}

fn query(request: *mut core::ffi::c_void, info: u32, index: &mut u32) -> Option<String> {
    let mut len = 0u32;
    unsafe { WinHttpQueryHeaders(request, info, null(), null_mut(), &mut len, index) };
    if len == 0 {
        return None;
    }

    let mut buf = vec![0u16; len as usize / 2 + 1];
    let ok = unsafe { WinHttpQueryHeaders(request, info, null(), buf.as_mut_ptr().cast(), &mut len, index) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize / 2]))
}

pub fn send(method: &str, url: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Result<Response, String> {
    let (host, path) = split(url)?;
    let session = session()?;

    unsafe {
        let connect = WinHttpConnect(session.0, wide(host).as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            return Err(format!("connect {host}: {}", std::io::Error::last_os_error()));
        }
        let connect = Handle(connect);

        let request = WinHttpOpenRequest(
            connect.0,
            wide(method).as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return Err(format!("open {url}: {}", std::io::Error::last_os_error()));
        }
        let request = Handle(request);

        let off = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_REDIRECTS;
        WinHttpSetOption(request.0, WINHTTP_OPTION_DISABLE_FEATURE, (&raw const off).cast(), 4);

        let text: String = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
        let text = wide(&text);

        check(
            WinHttpSendRequest(
                request.0,
                text.as_ptr(),
                u32::MAX,
                body.as_ptr().cast(),
                body.len() as u32,
                body.len() as u32,
                0,
            ),
            "send",
        )?;
        check(WinHttpReceiveResponse(request.0, null_mut()), "receive")?;

        let mut index = 0;
        let status = query(request.0, WINHTTP_QUERY_STATUS_CODE, &mut index)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let mut cookies = Vec::new();
        let mut index = 0;
        while let Some(cookie) = query(request.0, WINHTTP_QUERY_SET_COOKIE, &mut index) {
            cookies.push(cookie);
        }

        let mut out = Vec::new();
        loop {
            let mut available = 0u32;
            check(WinHttpQueryDataAvailable(request.0, &mut available), "read")?;
            if available == 0 {
                break;
            }

            let start = out.len();
            out.resize(start + available as usize, 0);
            let mut read = 0u32;
            check(
                WinHttpReadData(request.0, out[start..].as_mut_ptr().cast(), available, &mut read),
                "read",
            )?;
            out.truncate(start + read as usize);
        }

        Ok(Response {
            status,
            cookies,
            body: out,
        })
    }
}
