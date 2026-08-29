use std::fs::File;

use crate::{ContractError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

pub struct ProtocolLock {
    file: File,
}

impl ProtocolLock {
    #[cfg(unix)]
    pub fn acquire_at(
        parent: &File,
        name: &[u8],
        mode: LockMode,
        create: bool,
        nonblocking: bool,
    ) -> Result<Self> {
        use std::{ffi::CString, os::fd::FromRawFd, os::unix::fs::MetadataExt};

        crate::validate_child_name(name)?;
        let name = CString::new(name).map_err(|_| {
            ContractError::InvalidValue("lock name contains an interior NUL".into())
        })?;
        let mut flags = libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        if create {
            flags |= libc::O_CREAT | libc::O_EXCL;
        }
        // SAFETY: parent and name remain valid for the call; the returned fd is owned below.
        let descriptor = unsafe {
            libc::openat(
                std::os::fd::AsRawFd::as_raw_fd(parent),
                name.as_ptr(),
                flags,
                0o600,
            )
        };
        if descriptor < 0 {
            return Err(ContractError::Lock(std::io::Error::last_os_error()));
        }
        // SAFETY: openat returned a new owned descriptor.
        let file = unsafe { File::from_raw_fd(descriptor) };
        let metadata = file.metadata().map_err(ContractError::Lock)?;
        if !metadata.file_type().is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(ContractError::InvalidValue(
                "lock must be a user-owned regular file with mode 0600".into(),
            ));
        }
        if create {
            parent.sync_all().map_err(ContractError::Lock)?;
        }

        let mut operation = match mode {
            LockMode::Shared => libc::LOCK_SH,
            LockMode::Exclusive => libc::LOCK_EX,
        };
        if nonblocking {
            operation |= libc::LOCK_NB;
        }
        // SAFETY: flock only reads the valid descriptor and does not retain a pointer.
        if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), operation) } != 0 {
            return Err(ContractError::Lock(std::io::Error::last_os_error()));
        }

        let mut named = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: fstatat initializes named on success and does not retain pointers.
        if unsafe {
            libc::fstatat(
                std::os::fd::AsRawFd::as_raw_fd(parent),
                name.as_ptr(),
                named.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(ContractError::Lock(std::io::Error::last_os_error()));
        }
        // SAFETY: fstatat succeeded and initialized named.
        let named = unsafe { named.assume_init() };
        // dev_t is not u64 on every supported Unix target.
        #[allow(clippy::unnecessary_cast)]
        let named_device = named.st_dev as u64;
        if named.st_mode & libc::S_IFMT != libc::S_IFREG
            || named_device != metadata.dev()
            || named.st_ino != metadata.ino()
        {
            return Err(ContractError::InvalidValue(
                "lock pathname changed during acquisition".into(),
            ));
        }
        Ok(Self { file })
    }

    #[cfg(not(unix))]
    pub fn acquire_at(
        _parent: &File,
        _name: &[u8],
        _mode: LockMode,
        _create: bool,
        _nonblocking: bool,
    ) -> Result<Self> {
        Err(ContractError::InvalidValue(
            "maintenance protocol v1 supports Unix locks only".into(),
        ))
    }
}

impl Drop for ProtocolLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // SAFETY: flock only reads the descriptor, which remains open during Drop.
            let _ =
                unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.file), libc::LOCK_UN) };
        }
    }
}
