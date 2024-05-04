use std::{
    fs::File,
    ops::Deref,
    path::{Path, PathBuf},
};

use cdfs::{DirectoryEntry, ExtraAttributes, ISO9660};
use serde_json::json;

use super::{
    datetime_from_timestamp, ArchiveError, ArchiveFileEntity, ArchiveFileEntityType,
    ArchiveMetadata, Archived, DataSource, ExtractOptions, ListOptions,
};

pub struct ISOArchive<'a> {
    pub(crate) source: DataSource<'a>,
}

fn join_path_with_root<P: AsRef<Path>, S: Into<String>>(source: P, fs_path: S) -> PathBuf {
    source
        .as_ref()
        .components()
        .chain(PathBuf::from(fs_path.into()).components().skip(1))
        .collect::<PathBuf>()
}

impl ISOArchive<'_> {
    fn extract_dir(
        iso: &ISO9660<DataSource<'_>>,
        dest: &PathBuf,
        path: &str,
        _options: &ExtractOptions,
    ) -> Result<(), ArchiveError> {
        if let Some(DirectoryEntry::Directory(dir)) = iso.open(path)? {
            std::fs::create_dir_all(join_path_with_root(dest, path))?;

            for entry in dir.contents() {
                match entry? {
                    DirectoryEntry::File(file) => {
                        let path = join_path_with_root(dest, &file.identifier);
                        let mut copy_file = File::create(path)?;
                        let mut reader = file.read();
                        std::io::copy(&mut reader, &mut copy_file)?;
                    }
                    DirectoryEntry::Directory(dir) => {
                        let path = &dir.identifier;
                        let dest = join_path_with_root(dest, path);
                        Self::extract_dir(iso, &dest, path, _options)?;
                    }
                    DirectoryEntry::Symlink(link) => {
                        let path = &link.identifier;
                        let dest = join_path_with_root(dest, path);
                        if let Some(target) = link.target() {
                            let target = join_path_with_root(&dest, target);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(target, dest)?;
                            #[cfg(windows)]
                            std::os::windows::fs::symlink_file(target, dest)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn list_dir(
        iso: &ISO9660<DataSource<'_>>,
        cwd: &str,
        files: &mut Vec<ArchiveFileEntity>,
        options: &ListOptions,
    ) -> Result<(), ArchiveError> {
        let cwd_path = PathBuf::from(cwd);
        if let Some(DirectoryEntry::Directory(dir)) = iso.open(cwd)? {
            for entry in dir.contents() {
                match entry {
                    Ok(DirectoryEntry::File(file)) => {
                        let path = &file.identifier;
                        let size = file.size();
                        let entity = ArchiveFileEntity {
                            name: cwd_path.join(path).to_string_lossy().to_string(),
                            size: Some(size as u64),
                            compressed_size: Some(size as u64),
                            last_modified: datetime_from_timestamp(
                                file.modify_time().unix_timestamp(),
                            )
                            .ok(),
                            compression: None,
                            fstype: ArchiveFileEntityType::File,
                        };
                        files.push(entity);
                    }
                    Ok(DirectoryEntry::Directory(dir)) => {
                        if dir.identifier != "." && dir.identifier != ".." {
                            let path = cwd_path.join(&dir.identifier);

                            let entity = ArchiveFileEntity {
                                name: path.to_string_lossy().to_string(),
                                size: None,
                                compressed_size: None,
                                last_modified: datetime_from_timestamp(
                                    dir.modify_time().unix_timestamp(),
                                )
                                .ok(),
                                compression: None,
                                fstype: ArchiveFileEntityType::Directory,
                            };
                            files.push(entity);

                            Self::list_dir(iso, path.to_string_lossy().deref(), files, options)?;
                        }
                    }
                    Ok(DirectoryEntry::Symlink(link)) => {
                        let path = &link.identifier;

                        let entity = ArchiveFileEntity {
                            name: path.to_string(),
                            size: None,
                            compressed_size: None,
                            last_modified: datetime_from_timestamp(
                                link.modify_time().unix_timestamp(),
                            )
                            .ok(),
                            compression: None,
                            fstype: ArchiveFileEntityType::SymbolicLink,
                        };
                        files.push(entity);
                    }
                    Err(e) => {
                        options
                            .event_handler
                            .handle(super::ArchiveEvent::FailedToReadEntry(
                                cwd_path
                                    .join(PathBuf::from("???"))
                                    .to_string_lossy()
                                    .to_string(),
                                ArchiveError::Iso(e),
                            ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl<'a> Archived<'a> for ISOArchive<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Ok(Self { source })
    }

    fn extract(&self, options: super::ExtractOptions) -> Result<(), ArchiveError> {
        let dest = &options.destination;
        let iso = ISO9660::new(self.source.clone())?;

        Self::extract_dir(&iso, dest, "/", &options)?;

        Ok(())
    }

    fn list(&self, options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError> {
        let iso = ISO9660::new(self.source.clone())?;

        let mut acc = Vec::<ArchiveFileEntity>::new();
        Self::list_dir(&iso, &iso.root().identifier, &mut acc, &options)?;

        Ok(acc)
    }

    fn create(_options: super::CreateOptions) -> Result<super::CreateResult, ArchiveError> {
        Err(ArchiveError::UnsupportedActionForArchiveType(
            "create".to_string(),
            super::ArchiveType::Iso,
        ))
    }

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError> {
        let iso = ISO9660::new(self.source.clone())?;

        let mut acc = Vec::<ArchiveFileEntity>::new();

        Self::list_dir(
            &iso,
            &iso.root().identifier,
            &mut acc,
            &ListOptions::default(),
        )?;

        let (size, compressed_size) = acc.iter().fold((0, 0), |(s, cs), f| {
            (s + f.size.unwrap_or(0), cs + f.compressed_size.unwrap_or(0))
        });

        Ok(ArchiveMetadata {
            entries: acc,
            total_size: size,
            compressed_size,
            compression: None,
            additional: Some(json!(
                {
                    "is_rock_ridge": iso.is_rr(),
                    "block_size": iso.block_size() as u64,
                    "primary_volume_descriptor": iso.volume_set_identifier().to_string(),
                    "publisher_identifier": iso.publisher_identifier().to_string(),
                    "data_preparer_identifier": iso.data_preparer_identifier().to_string(),
                    "application_identifier": iso.application_identifier().to_string(),
                    "copyright_file_identifier":
                        iso.copyright_file_identifier(),
                    "abstract_file_identifier": iso.abstract_file_identifier(),
                    "bibliographic_file_identifier":
                        iso.bibliographic_file_identifier(),
                }
            )),
        })
    }

    fn open(&self, options: super::OpenOptions) -> Result<(), ArchiveError> {
        let iso = ISO9660::new(self.source.clone())?;

        let path = options.path.to_string_lossy().to_string();

        if let Some(DirectoryEntry::File(file)) = iso.open(&path)? {
            let mut reader = file.read();
            let mut writer = options.dest;
            std::io::copy(&mut reader, &mut writer)?;
            Ok(())
        } else {
            Err(ArchiveError::EntryNotFound(options.path))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn test_join_path_with_root() {
        let source = PathBuf::from("./Desktop");
        let fs_path = "/test";
        assert_eq!(
            super::join_path_with_root(source, fs_path),
            PathBuf::from("./Desktop/test")
        );
    }
}
