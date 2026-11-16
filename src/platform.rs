//! Runtime platform identification for Mysh.

use std::io;

/// Read the kernel hostname, preferring procfs and falling back to libc where
/// Android SELinux prevents direct access to the procfs node.
pub fn hostname() -> io::Result<String> {
    match std::fs::read_to_string("/proc/sys/kernel/hostname") {
        Ok(value) => validate_hostname(value),
        #[cfg(target_os = "android")]
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => hostname_from_libc(),
        Err(error) => Err(error),
    }
}

fn validate_hostname(value: String) -> io::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "kernel hostname is empty"));
    }
    Ok(value.to_owned())
}

/// Android SELinux can deny procfs reads; libc exposes the kernel hostname via gethostname().
#[cfg(target_os = "android")]
fn hostname_from_libc() -> io::Result<String> {
    use std::ffi::c_char;

    const HOSTNAME_CAPACITY: usize = 256;
    let mut buffer = [0 as c_char; HOSTNAME_CAPACITY];

    extern "C" {
        fn gethostname(name: *mut c_char, len: usize) -> i32;
    }

    // SAFETY: the pointer references a writable buffer of the supplied length.
    // Reserve the final byte for a NUL terminator. POSIX permits truncation
    // without writing a terminator when the supplied buffer is too small.
    let result = unsafe { gethostname(buffer.as_mut_ptr(), buffer.len() - 1) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    // The final byte was not exposed to gethostname, so the buffer always
    // contains a terminator within its bounds. Avoid an unbounded CStr read.
    let end = buffer.iter().position(|&byte| byte == 0).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "kernel hostname is not terminated")
    })?;
    let bytes: Vec<u8> = buffer[..end].iter().map(|&byte| byte as u8).collect();
    let value = std::str::from_utf8(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    validate_hostname(value.to_owned())
}

/// Return the C library family used by the target build.
pub const fn c_library() -> &'static str {
    if cfg!(target_os = "android") {
        "bionic"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "gnu") {
        "glibc"
    } else {
        "unknown-libc"
    }
}

/// Build the runtime identifier as `{hostname}-{c-library}-{arch}`.
pub fn runtime_identifier() -> io::Result<String> {
    let host = hostname()?;
    Ok(format!("{host}-{}-{}", c_library(), std::env::consts::ARCH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_is_available_from_kernel_interfaces() {
        let host = hostname().expect("kernel hostname should be available");
        assert!(!host.is_empty());
        assert!(!host.contains('\n'));
    }

    #[test]
    fn hostname_validation_rejects_empty_values() {
        assert_eq!(
            validate_hostname("  \n".to_owned()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn c_library_matches_target_configuration() {
        let expected = if cfg!(target_os = "android") {
            "bionic"
        } else if cfg!(target_env = "musl") {
            "musl"
        } else if cfg!(target_env = "gnu") {
            "glibc"
        } else {
            "unknown-libc"
        };
        assert_eq!(c_library(), expected);
    }

    #[test]
    fn runtime_identifier_contains_hostname_library_and_architecture() {
        let value = runtime_identifier().expect("runtime identifier should be available");
        let host = hostname().expect("hostname should be available");
        assert_eq!(value, format!("{host}-{}-{}", c_library(), std::env::consts::ARCH));
    }
}
