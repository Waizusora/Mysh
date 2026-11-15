//! Runtime platform identification for Mysh.

use std::io;

/// Read the kernel hostname exposed by procfs.
pub fn hostname() -> io::Result<String> {
    let value = std::fs::read_to_string("/proc/sys/kernel/hostname")?;
    let value = value.trim();

    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "kernel hostname is empty",
        ));
    }

    Ok(value.to_owned())
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
    fn hostname_is_read_from_kernel_procfs() {
        let host = hostname().expect("procfs kernel hostname should be readable");
        assert!(!host.is_empty());
        assert!(!host.contains('\n'));
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
        assert_eq!(
            value,
            format!("{host}-{}-{}", c_library(), std::env::consts::ARCH)
        );
    }
}
