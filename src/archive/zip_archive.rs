use std::{
    collections::HashSet,
    fs::File,
    io::{BufWriter, Error, ErrorKind, Read},
    path::PathBuf,
};

use byte_unit::{Byte, UnitType};
use serde_json::json;
use zip::{result::ZipError, write::FileOptions, ZipWriter};

use crate::archive::{
    codecs::ArchiveCompression, datetime_from_timestamp, ArchiveError, ArchiveEvent,
    ArchiveFileEntity, ArchiveFileEntityType, Archived, CreateOptions, CreateResult, DataSource,
    EventHandler, ExtractOptions, ListOptions, ReadSeek, SkipReason, DEFAULT_BUF_SIZE,
};

use super::ArchiveMetadata;

pub struct ZipArchive<'a> {
    pub(crate) source: DataSource<'a>,
}

impl<'a> ZipArchive<'a> {
    fn reader(&'a self) -> Result<Box<dyn ReadSeek + 'a>, Error> {
        match &self.source {
            DataSource::File(file, _) => Ok(Box::new(file.try_clone()?)),
            DataSource::Stream(val) => Ok(Box::new(val.clone())),
        }
    }
}

impl<'a> Archived<'a> for ZipArchive<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Ok(Self { source })
    }

    fn extract(&self, options: ExtractOptions) -> Result<(), ArchiveError> {
        use std::fs;

        let reader = self.reader()?;
        let mut zip = zip::ZipArchive::new(reader)?;

        let files = options
            .files
            .clone()
            .map(|f| f.into_iter().collect::<HashSet<_>>());

        for i in 0..zip.len() {
            let mut file = match &options.password {
                None => zip.by_index(i).map_err(ArchiveError::Zip),
                Some(p) => match zip.by_index_decrypt(i, p.as_bytes()) {
                    Ok(Ok(f)) => Ok(f),
                    Ok(Err(e)) => Err(ArchiveError::Password(e)),
                    Err(e) => Err(ArchiveError::Zip(e)),
                },
            }?;
            if let Some(files) = &files {
                if !files.contains(file.name()) {
                    continue;
                }
            }
            let filepath = file
                .enclosed_name()
                .ok_or(ArchiveError::Zip(ZipError::FileNotFound))?;

            let outpath = options.destination.join(filepath);

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
                options.handle(ArchiveEvent::Created(
                    outpath.to_string_lossy().to_string(),
                    ArchiveFileEntityType::Directory,
                ));
            } else {
                options.handle(ArchiveEvent::Extracting(
                    outpath.to_string_lossy().to_string(),
                    Some(file.size()),
                ));

                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                if outpath.exists() {
                    if options.overwrite {
                        fs::remove_file(&outpath)?;
                    } else {
                        // yellow in ansi
                        options.handle(ArchiveEvent::Skipped(
                            outpath.to_string_lossy().to_string(),
                            SkipReason::AlreadyExists,
                        ));
                        continue;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
            // Get and Set permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                }
            }
        }
        options.handle(ArchiveEvent::DoneExtracting(
            self.source.as_ref().to_string(),
            options.destination.to_string_lossy().to_string(),
        ));
        Ok(())
    }

    fn list(&self, _options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError> {
        let reader = self.reader()?;

        let mut zip = zip::ZipArchive::new(reader)?;

        let entities = (0..zip.len())
            .map(|i| {
                let file = zip.by_index(i)?;

                let name = file
                    .enclosed_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let last_modified = file
                    .last_modified()
                    .to_time()
                    .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;

                let tpe = if file.is_dir() {
                    ArchiveFileEntityType::Directory
                } else if file.is_file() {
                    ArchiveFileEntityType::File
                } else {
                    ArchiveFileEntityType::Unknown
                };

                let (size, compressed_size) = if tpe == ArchiveFileEntityType::File {
                    (Some(file.size()), (Some(file.compressed_size())))
                } else {
                    (None, None)
                };

                let entity: ArchiveFileEntity = ArchiveFileEntity {
                    name,
                    size,
                    compressed_size,
                    fstype: tpe,
                    last_modified: datetime_from_timestamp(last_modified.unix_timestamp()).ok(),
                    compression: Some(file.compression().to_string()),
                };

                Ok(entity)
            })
            .collect::<Result<Vec<_>, ArchiveError>>();

        entities
    }

    fn create(options: CreateOptions) -> Result<CreateResult, ArchiveError> {
        const DEFAULT_COMPRESSION: ArchiveCompression = ArchiveCompression::Gzip;

        let dest = options.destination;
        let files = options.files;
        let allow_hidden = options.include_hidden;
        let compression = zip::CompressionMethod::try_from(
            options.archive_compression.unwrap_or(DEFAULT_COMPRESSION),
        )?;

        eprintln!(
            "Creating zip archive at {} using compression method {}.",
            dest.display(),
            compression
        );

        let file = File::create(&dest)?;
        let buf_writer = BufWriter::with_capacity(DEFAULT_BUF_SIZE, file);

        let mut zip = ZipWriter::new(buf_writer);

        let mut total_size = 0;

        for path in files {
            let metadata = std::fs::metadata(&path)?;

            let name = path
                .strip_prefix(&options.source)
                .as_deref()
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .to_string();

            let options = FileOptions::default()
                .compression_method(compression)
                .compression_level(None);

            if metadata.is_dir() {
                eprintln!("Adding directory: {}", name);
                zip.add_directory(&name, options)?;
            } else {
                eprintln!(
                    "Adding file: {} ({})",
                    name,
                    Byte::from(metadata.len()).get_appropriate_unit(UnitType::Both)
                );
                // check first if the file is hidden
                let is_hidden = {
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
                        metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
                    }
                    #[cfg(not(windows))]
                    {
                        name.starts_with('.')
                    }
                };
                if !allow_hidden && is_hidden {
                    continue;
                }

                // max size is 4GB
                zip.start_file(&name, options.large_file(metadata.len() > u32::MAX as u64))?;

                let mut file = File::open(&path)?;

                let size = std::io::copy(&mut file, &mut zip)?;
                total_size += size;
            }
        }
        zip.finish()?;

        eprintln!(
            "Done creating zip archive: {} ({})",
            dest.display(),
            Byte::from(total_size).get_appropriate_unit(UnitType::Both)
        );

        Ok(CreateResult {
            path: PathBuf::from(&dest),
            total_size,
            compressed_size: std::fs::metadata(dest)?.len(),
        })
    }

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError> {
        let mut reader = self.reader()?;
        let len = reader.seek(std::io::SeekFrom::End(0))?;
        let zip = zip::ZipArchive::new(reader)?;
        let mut str = String::new();
        let comment = zip.comment().read_to_string(&mut str).map(|_| str);

        let entries = self.list(ListOptions::default())?;

        Ok(ArchiveMetadata {
            total_size: entries.iter().filter_map(|e| e.size).sum(),
            compressed_size: len,
            compression: None,
            entries,
            additional: Some(json!(
                {
                    "comment": comment.ok(),
                }
            )),
        })
    }

    fn open(&'a self, options: super::OpenOptions) -> Result<(), ArchiveError> {
        let reader = self.reader()?;
        let mut zip = zip::ZipArchive::new(reader)?;

        let path_str = options.path.to_string_lossy().to_string();

        let mut file = match &options.password {
            None => zip.by_name(path_str.as_str()).map_err(ArchiveError::Zip),
            Some(p) => match zip.by_name_decrypt(path_str.as_str(), p.as_bytes()) {
                Ok(Ok(f)) => Ok(f),
                Ok(Err(e)) => Err(ArchiveError::Password(e)),
                Err(e) => Err(ArchiveError::Zip(e)),
            },
        }?;

        let mut writer = options.dest;

        std::io::copy(&mut file, &mut writer)?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use std::str::FromStr;

    use chrono::{DateTime, FixedOffset};

    use crate::assert_none;

    use super::*;

    // if feature zip and feature deflate_codecs
    #[cfg(all(feature = "zip_archive", feature = "deflate_codecs"))]
    #[test]
    fn test_list_zip() {
        use crate::assert_eq_some;

        let archive_path = "tests/fixtures/test1.zip";
        let archive = ZipArchive::from_path(archive_path).unwrap();
        let entities = archive.list(ListOptions::default()).unwrap();

        assert_eq!(entities.len(), 3);

        let entity = &entities[0];
        assert_eq!(entity.name, "test1/dir1/");
        assert_none!(entity.size);
        assert_none!(entity.compressed_size);
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
