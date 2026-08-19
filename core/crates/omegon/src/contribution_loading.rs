use std::{fs::File, path::Path};

use omegon_maintenance_contracts::{
    AuthorityKey, ContributionAdmissionGuard, ContributionKind, MaintenanceStateV1,
};

pub(crate) struct GuardedContributionDirectory {
    directory: File,
    admission: ContributionAdmissionGuard,
}

impl GuardedContributionDirectory {
    #[cfg(unix)]
    pub(crate) fn open(
        root_path: &Path,
        components: &[&[u8]],
        home_path: &Path,
        kind: ContributionKind,
        scope: &str,
    ) -> anyhow::Result<Option<Self>> {
        let root = omegon_maintenance_contracts::open_secure_root(root_path)?;
        let Some(directory) = open_relative_directory(&root, components)? else {
            return Ok(None);
        };
        let parent_identity = omegon_maintenance_contracts::path_identity(&directory)?;
        ensure_home_exists(home_path)?;
        let home = omegon_maintenance_contracts::open_secure_root(home_path)?;
        let home_identity = omegon_maintenance_contracts::path_identity(&home)?;
        let state = MaintenanceStateV1::bootstrap(
            &home,
            home_identity,
            &uuid::Uuid::new_v4().to_string(),
            false,
        )?;
        let admission = state.admit_contribution_scope(
            kind,
            scope,
            &parent_identity,
            &uuid::Uuid::new_v4().to_string(),
            false,
        )?;
        Ok(Some(Self {
            directory,
            admission,
        }))
    }

    #[cfg(not(unix))]
    pub(crate) fn open(
        _root_path: &Path,
        _components: &[&[u8]],
        _home_path: &Path,
        _kind: ContributionKind,
        _scope: &str,
    ) -> anyhow::Result<Option<Self>> {
        anyhow::bail!("guarded contribution loading requires Unix")
    }

    pub(crate) fn scope_key(&self) -> AuthorityKey {
        self.admission.scope_key
    }

    pub(crate) fn allows(&self, raw_name: &[u8]) -> anyhow::Result<bool> {
        self.admission.allows(raw_name).map_err(Into::into)
    }

    pub(crate) fn entry_names(&self, limit: usize) -> anyhow::Result<Vec<Vec<u8>>> {
        read_directory_names(&self.directory, limit)
    }

    pub(crate) fn read_file(
        &self,
        raw_name: &[u8],
        limit: usize,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        read_file_at(&self.directory, raw_name, limit)
    }

    pub(crate) fn open_child_directory(&self, raw_name: &[u8]) -> anyhow::Result<Option<File>> {
        open_child_directory(&self.directory, raw_name)
    }
}

#[cfg(unix)]
pub(crate) fn read_file_at(
    parent: &File,
    name: &[u8],
    limit: usize,
) -> anyhow::Result<Option<Vec<u8>>> {
    use std::{ffi::CString, io::Read, os::fd::FromRawFd};

    omegon_maintenance_contracts::validate_child_name(name)?;
    let name = CString::new(name)?;
    // SAFETY: parent/name are valid for this call; the returned descriptor is owned below.
    let descriptor = unsafe {
        libc::openat(
            std::os::fd::AsRawFd::as_raw_fd(parent),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        return if error.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error.into())
        };
    }
    // SAFETY: openat returned a new owned descriptor.
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    if !file.metadata()?.is_file() {
        anyhow::bail!("contribution entry is not a regular file");
    }
    let before = omegon_maintenance_contracts::file_identity(&file)?;
    if before.size > limit as u64 {
        anyhow::bail!("contribution file exceeds the {limit}-byte limit");
    }
    let mut bytes = Vec::with_capacity(before.size as usize);
    file.by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    let after = omegon_maintenance_contracts::file_identity(&file)?;
    if bytes.len() > limit || before != after {
        anyhow::bail!("contribution file exceeded its limit or changed during read");
    }
    Ok(Some(bytes))
}

#[cfg(not(unix))]
pub(crate) fn read_file_at(
    _parent: &File,
    _name: &[u8],
    _limit: usize,
) -> anyhow::Result<Option<Vec<u8>>> {
    anyhow::bail!("guarded contribution loading requires Unix")
}

#[cfg(unix)]
fn ensure_home_exists(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    if !path.exists()
        && let Err(error) = std::fs::DirBuilder::new().mode(0o700).create(path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(unix)]
fn open_relative_directory(root: &File, components: &[&[u8]]) -> anyhow::Result<Option<File>> {
    let mut current = root.try_clone()?;
    for component in components {
        let Some(next) = open_child_directory(&current, component)? else {
            return Ok(None);
        };
        current = next;
    }
    Ok(Some(current))
}

#[cfg(unix)]
fn open_child_directory(parent: &File, name: &[u8]) -> anyhow::Result<Option<File>> {
    use std::{ffi::CString, os::fd::FromRawFd};

    omegon_maintenance_contracts::validate_child_name(name)?;
    let name = CString::new(name)?;
    // SAFETY: parent/name are valid for this call; the returned descriptor is owned below.
    let descriptor = unsafe {
        libc::openat(
            std::os::fd::AsRawFd::as_raw_fd(parent),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        return if error.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error.into())
        };
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(Some(unsafe { File::from_raw_fd(descriptor) }))
}

#[cfg(not(unix))]
fn open_child_directory(_parent: &File, _name: &[u8]) -> anyhow::Result<Option<File>> {
    anyhow::bail!("guarded contribution loading requires Unix")
}

#[cfg(unix)]
fn read_directory_names(directory: &File, limit: usize) -> anyhow::Result<Vec<Vec<u8>>> {
    use std::{ffi::CStr, os::fd::AsRawFd};

    // SAFETY: dup returns a new descriptor consumed by fdopendir.
    let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: duplicate is an owned directory descriptor.
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        // SAFETY: fdopendir did not consume duplicate on failure.
        unsafe { libc::close(duplicate) };
        return Err(std::io::Error::last_os_error().into());
    }
    let mut entries = Vec::new();
    loop {
        clear_errno();
        // SAFETY: stream remains valid until it is closed below.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            let error = current_errno();
            // SAFETY: stream is a live DIR pointer and closed exactly once.
            unsafe { libc::closedir(stream) };
            return if error == 0 {
                Ok(entries)
            } else {
                Err(std::io::Error::from_raw_os_error(error).into())
            };
        }
        // SAFETY: d_name is NUL-terminated for a successful readdir result.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        entries.push(name.to_vec());
        if entries.len() > limit {
            // SAFETY: stream is a live DIR pointer and closed exactly once.
            unsafe { libc::closedir(stream) };
            anyhow::bail!("contribution directory exceeds {limit} entries");
        }
    }
}

#[cfg(not(unix))]
fn read_directory_names(_directory: &File, _limit: usize) -> anyhow::Result<Vec<Vec<u8>>> {
    anyhow::bail!("guarded contribution loading requires Unix")
}

#[cfg(target_os = "macos")]
fn clear_errno() {
    // SAFETY: __error returns the calling thread's errno pointer.
    unsafe { *libc::__error() = 0 };
}

#[cfg(target_os = "macos")]
fn current_errno() -> i32 {
    // SAFETY: __error returns the calling thread's errno pointer.
    unsafe { *libc::__error() }
}

#[cfg(target_os = "linux")]
fn clear_errno() {
    // SAFETY: __errno_location returns the calling thread's errno pointer.
    unsafe { *libc::__errno_location() = 0 };
}

#[cfg(target_os = "linux")]
fn current_errno() -> i32 {
    // SAFETY: __errno_location returns the calling thread's errno pointer.
    unsafe { *libc::__errno_location() }
}
