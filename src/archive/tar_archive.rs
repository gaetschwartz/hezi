use std::{
    collections::HashSet,
    fs::File,
    io::{BufReader, Read, Write},
};

use byte_unit::{Byte, UnitType};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tar;

use crate::archive::{
    codecs::{ArchiveCodec, ArchiveCompression, FinishableWrite},
    datetime_from_timestamp, ArchiveError, ArchiveFileEntity, ArchiveFileEntityType,
    ArchiveMetadata, ArchiveType, Archived, AsTarArchiveResult, CreateOptions, CreateResult,
    DataSource, EventHandler, ExtractOptions, ListOptions, MagicBytesHex,
};

pub struct TarArchive<'a> {
    pub(crate) source: DataSource<'a>,
}

impl<'a> TarArchive<'a> {
    fn reader(&'a self) -> Result<Box<dyn std::io::Read + 'a>, ArchiveError> {
        let compression = ArchiveType::try_from_datasource(self.source.clone())?.1;

        ArchiveCodec::get_reader(self.source.clone(), &compression)
    }

    fn writer<'w, R: Write + 'w>(
        tar_compression: &ArchiveCompression,
        writer: R,
    ) -> Result<Box<dyn FinishableWrite + 'w>, ArchiveError> {
        ArchiveCodec::get_writer(tar_compression, writer)
    }
}

impl<'a> Archived<'a> for TarArchive<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Ok(Self { source })
    }

    fn extract(&self, options: ExtractOptions) -> Result<(), ArchiveError> {
        use std::fs;
        let reader = self.reader()?;
        let mut archive = tar::Archive::new(reader);

        let files = options
            .files
            .clone()
            .map(|f| f.into_iter().collect::<HashSet<_>>());

        if options.destination.symlink_metadata().is_err() {
            fs::create_dir_all(&options.destination)?;
        }

        // Canonicalizing the dst directory will prepend the path with '\\?\'
        // on windows which will allow windows APIs to treat the path as an
        // extended-length path with a 32,767 character limit. Otherwise all
        // unpacked paths over 260 characters will fail on creation with a
        // NotFound exception.
        let dst = &options
            .destination
            .canonicalize()
            .unwrap_or(options.destination.to_path_buf());

        // Delay any directory entries until the end (they will be created if needed by
        // descendants), to ensure that directory permissions do not interfer with descendant
        // extraction.
        let mut directories = Vec::new();
        for entry in archive.entries()? {
            let mut file = entry?;

            let file_path: String = file.path().map(|p| p.to_string_lossy().to_string())?;

            if let Some(files) = &files {
                if !files.contains(&file_path) {
                    continue;
                }
            }
            if file.header().entry_type() == tar::EntryType::Directory {
                let path = dst.join(file_path);
                directories.push(file);
                options.handle(crate::archive::ArchiveEvent::Created(
                    path.to_string_lossy().to_string(),
                    crate::archive::ArchiveFileEntityType::Directory,
                ));
            } else {
                file.unpack_in(dst)?;
                options.handle(crate::archive::ArchiveEvent::Extracting(
                    file_path,
                    file.size().into(),
                ));
            }
        }
        for mut dir in directories {
            dir.unpack_in(dst)?;
            let dir_path = dir.path().map(|p| p.to_string_lossy().to_string())?;
            options.handle(crate::archive::ArchiveEvent::Extracting(dir_path, None));
        }

        options.handle(crate::archive::ArchiveEvent::DoneExtracting(
            self.source.as_ref().to_string(),
            dst.to_string_lossy().to_string(),
        ));
        Ok(())
    }

    fn list(&self, _options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError> {
        // println!("list tar archive");
        // read the file to identify the archive type
        let reader = self.reader()?;

        let compression = ArchiveType::try_from_datasource(self.source.clone())?.1;
        // println!("compression: {:?}", compression);

        let mut archive = tar::Archive::new(reader);

        let entities = archive
            .entries()?
            .map(|entry| {
                let entry = entry?;
                let fstype = entry.header().entry_type().into();

                let (size, compressed_size) = if fstype == ArchiveFileEntityType::File {
                    (Some(entry.size()), Some(entry.size()))
                } else {
                    (None, None)
                };
                Ok(ArchiveFileEntity {
                    name: entry
                        .path()?
                        .to_string_lossy()
                        .to_string()
                        .replace('\\', "/"),
                    size,
                    compressed_size,
                    fstype,
                    last_modified: entry
                        .header()
                        .mtime()
                        .map(|t| t as i64)
                        .and_then(datetime_from_timestamp)
                        .ok(),
                    compression: Some(compression.to_string()),
                })
            })
            .collect::<Result<Vec<_>, ArchiveError>>();

        entities
    }

    fn create(options: CreateOptions) -> Result<CreateResult, ArchiveError> {
        let compression = options
            .archive_compression
            .ok_or_else(|| ArchiveError::CompressionMethodRequired)?;

        eprintln!(
            "Creating tar archive at {} with compression {} and source {}",
            options.destination.display(),
            compression,
            options.source.display()
        );

        let writer = File::create(&options.destination).map_err(|e| {
            ArchiveError::Io(std::io::Error::new(
                e.kind(),
                format!("could not create destination file: {}", e),
            ))
        })?;

        let enc_writer = Self::writer(&compression, &writer)?;

        let mut archive = tar::Builder::new(enc_writer);
        let mut total_size = 0;

        let files = options
            .files
            .par_iter()
            .map(|f| {
                let metadata = std::fs::metadata(f).map_err(|e| {
                    ArchiveError::Io(std::io::Error::new(
                        e.kind(),
                        format!("could not read file metadata for '{}': {}", f.display(), e),
                    ))
                })?;

                let mut name = f
                    .strip_prefix(&options.source)
                    .as_deref()
                    .map_or_else(|_| f.to_path_buf(), |p| p.to_path_buf());
                if metadata.is_dir() && name.as_os_str().is_empty() {
                    name.push(".");
                }
                Ok((f, name, metadata))
            })
            .collect::<Result<Vec<_>, ArchiveError>>()
            .map_err(|e| {
                ArchiveError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to read file metadatas: {}", e),
                ))
            })?;

        for (file, name, metadata) in files {
            total_size += metadata.len();

            if metadata.is_file() {
                eprintln!(
                    "Adding: {} -> {} ({})",
                    file.display(),
                    name.display(),
                    Byte::from(metadata.len()).get_appropriate_unit(UnitType::Both)
                );
            } else {
                eprintln!("Adding: {} -> {}", file.display(), name.display());
            }
            archive
                .append_path_with_name(file, name)
                .into_tar_archive_result()?;
        }

        let mut moved = archive.into_inner()?;
        moved.finish_writer()?;

        let size = writer.metadata()?.len();

        eprintln!(
            "Done creating tar archive: {} ({})",
            options.destination.display(),
            Byte::from(size).get_appropriate_unit(UnitType::Both)
        );

        Ok(CreateResult {
            path: options.destination,
            total_size,
            compressed_size: size,
        })
    }

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError> {
        let entries = self.list(ListOptions::default())?;

        let (size, compressed_size) = entries.iter().fold((0, 0), |(s, cs), e| {
            (s + e.size.unwrap_or(0), cs + e.compressed_size.unwrap_or(0))
        });

        Ok(ArchiveMetadata {
            entries,
            total_size: size,
            compressed_size,
            compression: ArchiveType::try_from_datasource(self.source.clone())
                .ok()
                .map(|t| t.1),
            additional: None,
        })
    }

    fn open(&'a self, options: crate::archive::OpenOptions) -> Result<(), ArchiveError> {
        let path = options.path;

        let reader = self.reader()?;

        let mut archive = tar::Archive::new(reader);

        let mut file = archive
            .entries()?
            .find_map(|entry| {
                let entry = entry.ok()?;
                let entry_path = entry.path().ok()?;
                if entry_path == path {
                    Some(entry)
                } else {
                    None
                }
            })
            .ok_or_else(|| ArchiveError::EntryNotFound(path))?;

        let mut writer = options.dest;

        std::io::copy(&mut file, &mut writer)?;

        Ok(())
    }
}

impl<'a> TryFrom<DataSource<'a>> for ArchiveCompression {
    fn try_from(source: DataSource<'a>) -> Result<Self, Self::Error> {
        let mut reader = BufReader::new(source);

        // read magic bytes to identify the compression
        let mut magic_bytes = [0; 8];
        reader.read_exact(&mut magic_bytes)?;
        Self::try_from(MagicBytesHex::new(0, magic_bytes))
    }

    type Error = std::io::Error;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::str::FromStr;

    use chrono::{DateTime, FixedOffset};

    use crate::{archive::ArchiveFileEntityType, assert_eq_some};

    use super::*;

    // skip this test for now
    #[ignore]
    #[test]
    fn list_tar_archive() {
        let archive_path = "tests/fixtures/test1.tar.gz";
        let archive = TarArchive::from_path(archive_path).unwrap();
        let entities = archive.list(ListOptions::default()).unwrap();

        assert_eq!(entities.len(), 3);

        let entity = &entities[0];
        assert_eq!(entity.name, "test1/dir1/");
        assert_eq_some!(entity.size, 0);
        assert_eq_some!(entity.compressed_size, 0);
        assert_eq!(entity.fstype, ArchiveFileEntityType::Directory);
        assert_eq_some!(entity.compression, "Stored".to_string());
        assert_eq!(
            entity.last_modified,
            // rfc3339 format
            Some(DateTime::<FixedOffset>::from_str("2023-10-01T16:33:52+00:00").unwrap())
        );

        let entity = &entities[1];
        assert_eq!(entity.name, "test1/dir1/file2.txt");
        assert_eq_some!(entity.size, 444);
        assert_eq_some!(entity.compressed_size, 263);
        assert_eq!(entity.fstype, ArchiveFileEntityType::File);
        assert_eq_some!(entity.compression, "Deflated".to_string());
        assert_eq!(
            entity.last_modified,
            Some(DateTime::<FixedOffset>::from_str("2023-10-01T16:47:24+00:00").unwrap())
        );

        let entity = &entities[2];
        assert_eq!(entity.name, "test1/file1.txt");
        assert_eq_some!(entity.size, 1510);
        assert_eq_some!(entity.compressed_size, 52);
        assert_eq!(entity.fstype, ArchiveFileEntityType::File);
        assert_eq_some!(entity.compression, "Deflated".to_string());
        assert_eq!(
            entity.last_modified,
            Some(DateTime::<FixedOffset>::from_str("2023-10-01T16:46:52+00:00").unwrap())
        );
    }
}
