//! Private regular files stay named and open until the native player releases them.

use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

const CREATE_ATTEMPTS: usize = 8;

pub(super) struct SourceFile {
    file: Option<File>,
    path: PathBuf,
    temporary: bool,
}

impl SourceFile {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Self::from_file(file, path.to_owned())
    }
    pub(super) fn from_file(file: File, path: PathBuf) -> io::Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(io::Error::other(
                "Video source must be a nonempty regular file",
            ));
        }
        Ok(Self {
            file: Some(file),
            path,
            temporary: false,
        })
    }
    #[cfg(test)]
    fn create(directory: &Path, bytes: &[u8], live: impl Fn() -> bool) -> io::Result<Self> {
        Self::copy(directory, io::Cursor::new(bytes), live)
    }
    pub(super) fn copy(
        directory: &Path,
        mut reader: impl Read,
        live: impl Fn() -> bool,
    ) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Video needs an absolute Ability cache directory",
            ));
        }
        for _ in 0..CREATE_ATTEMPTS {
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(io::Error::other)?;
            let path = directory.join(format!("nsh-video-{:032x}", u128::from_le_bytes(random)));
            let file = match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let mut source = Self {
                file: Some(file),
                path,
                temporary: true,
            };
            let mut buffer = [0; 64 << 10];
            loop {
                if !live() {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Video source copy was cancelled",
                    ));
                }
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                source.file.as_mut().unwrap().write_all(&buffer[..count])?;
            }
            if source.file().metadata()?.len() == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Video source is empty",
                ));
            }
            source.file.as_mut().unwrap().rewind()?;
            return Ok(source);
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Video cache filename collisions exhausted the retry limit",
        ))
    }

    pub(super) fn file(&self) -> &File {
        self.file.as_ref().unwrap()
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if self.temporary
            && let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            log::warn!("Video source cleanup failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, io::Read, os::unix::fs::PermissionsExt};

    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = std::env::var_os("NSH_VIDEO_TEST_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let mut random = [0; 16];
            getrandom::fill(&mut random).unwrap();
            let path = root.join(format!(
                "nsh-video-test-{:032x}",
                u128::from_le_bytes(random)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn local_sources_keep_the_original_file_after_player_retirement() {
        let directory = Directory::new();
        let path = directory.0.join("movie.mp4");
        std::fs::write(&path, b"original").unwrap();
        let source = SourceFile::open(&path).unwrap();
        assert_eq!(source.file().metadata().unwrap().len(), 8);
        assert!(!source.temporary);
        drop(source);
        assert_eq!(std::fs::read(path).unwrap(), b"original");
    }
    #[test]
    fn large_sources_stream_to_disk_without_a_full_memory_buffer() {
        let directory = Directory::new();
        let size = (64 << 20) + 7;
        let reader = std::io::repeat(17).take(size);
        let source = SourceFile::copy(&directory.0, reader, || true).unwrap();
        assert_eq!(source.file().metadata().unwrap().len(), size);
        let path = source.path.clone();
        drop(source);
        assert!(!path.exists());
    }
    #[test]
    fn named_regular_file_preserves_exact_bytes_and_starts_at_zero() {
        let directory = Directory::new();
        let bytes = b"\0\0\0\x18ftypmp42\0binary\xffpayload";
        let source = SourceFile::create(&directory.0, bytes, || true).unwrap();
        let metadata = source.file().metadata().unwrap();
        assert!(metadata.is_file());
        assert_eq!(metadata.len(), bytes.len() as u64);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert!(source.path.is_absolute());
        assert_eq!(source.path.parent(), Some(directory.0.as_path()));
        let mut file = source.file();
        let mut actual = Vec::new();
        file.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes);
        assert_eq!(std::fs::read(&source.path).unwrap(), bytes);
    }

    #[test]
    fn source_lives_until_owner_drop_then_file_is_removed() {
        let directory = Directory::new();
        let source = SourceFile::create(&directory.0, b"payload", || true).unwrap();
        let path = source.path.clone();
        assert!(path.exists());
        drop(source);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn cancellation_after_a_partial_write_removes_the_file() {
        let directory = Directory::new();
        let calls = Cell::new(0);
        let result = SourceFile::create(&directory.0, &vec![7; 128 << 10], || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                let path = std::fs::read_dir(&directory.0)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                assert_eq!(std::fs::metadata(path).unwrap().len(), 64 << 10);
                false
            } else {
                true
            }
        });
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Interrupted);
        assert_eq!(calls.get(), 2);
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn concurrent_sources_have_distinct_exclusive_files() {
        let directory = Directory::new();
        let a = SourceFile::create(&directory.0, b"first", || true).unwrap();
        let b = SourceFile::create(&directory.0, b"second", || true).unwrap();
        assert_ne!(a.path, b.path);
        assert_eq!(std::fs::read(&a.path).unwrap(), b"first");
        assert_eq!(std::fs::read(&b.path).unwrap(), b"second");
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
    }

    #[test]
    fn invalid_sources_and_relative_directories_create_nothing() {
        let directory = Directory::new();
        assert_eq!(
            SourceFile::create(Path::new("relative"), b"payload", || true)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            SourceFile::create(&directory.0, &[], || true)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}
