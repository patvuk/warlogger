#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::send;
#[cfg(windows)]
pub use windows::send;

pub struct Response {
    pub status: u16,
    pub cookies: Vec<String>,
    pub body: Vec<u8>,
}
