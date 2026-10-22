use super::wide;
use std::ptr::null_mut;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW, CredWriteW,
};

fn target(account: &str) -> Vec<u16> {
    wide(&format!("warlogger:{account}"))
}

pub fn save_secret(account: &str, secret: &str) -> Result<(), String> {
    let mut target = target(account);
    let mut user = wide(account);
    let mut blob = secret.as_bytes().to_vec();

    let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = target.as_mut_ptr();
    cred.UserName = user.as_mut_ptr();
    cred.CredentialBlobSize = blob.len() as u32;
    cred.CredentialBlob = blob.as_mut_ptr();
    cred.Persist = CRED_PERSIST_LOCAL_MACHINE;

    match unsafe { CredWriteW(&cred, 0) } {
        0 => Err(format!(
            "saving the password failed: {}",
            std::io::Error::last_os_error()
        )),
        _ => Ok(()),
    }
}

#[cfg(not(feature = "portable"))]
pub fn forget_all() {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CredEnumerateW};

    let filter = wide("warlogger:*");
    let (mut count, mut list) = (0u32, null_mut());

    unsafe {
        if CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut list) == 0 {
            return;
        }
        for &cred in std::slice::from_raw_parts(list, count as usize) {
            CredDeleteW((*cred).TargetName, CRED_TYPE_GENERIC, 0);
        }
        CredFree(list.cast());
    }
}

pub fn load_secret(account: &str) -> Option<String> {
    let target = target(account);
    let mut cred: *mut CREDENTIALW = null_mut();

    unsafe {
        if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) == 0 {
            return None;
        }

        let blob = std::slice::from_raw_parts((*cred).CredentialBlob, (*cred).CredentialBlobSize as usize);
        let secret = String::from_utf8(blob.to_vec()).ok();
        CredFree(cred.cast());
        secret
    }
}
