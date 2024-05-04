use std::{
    collections::HashSet,
    fs::File,
    io::{BufWriter, Error, Read, Write},
};

use super::{
    datetime_from_timestamp, ArchiveError, ArchiveEvent, ArchiveFileEntity, ArchiveFileEntityType,
    ArchiveMetadata, Archived, CreateOptions, CreateResult, DataSource, EventHandler,
    ExtractOptions, Lengthed, ListOptions, SimpleLogger, SkipReason, DEFAULT_BUF_SIZE,
};
use byte_unit::Byte;
use sevenz_rust::{BlockDecoder, Password, SevenZArchiveEntry, SevenZMethod, SevenZReader};

#[cfg(feature = "lzma_codecs")]
use sevenz_rust::SevenZWriter;

pub struct SevenZArchive<'a> {
    pub(crate) source: DataSource<'a>,
}

impl<'a> SevenZArchive<'a> {
    #[inline]
    fn reader(&'a self) -> Result<DataSource<'a>, Error> {
        self.source.try_clone()
    }
}

impl<'a> Archived<'a> for SevenZArchive<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Ok(Self { source })
    }

    fn extract(&self, options: ExtractOptions) -> Result<(), ArchiveError> {
        let reader = self.reader()?;
        let reader_len: u64 = reader.len()?;
        let mut sz = SevenZReader::new(
            reader,
            // reader_len: u64
            reader_len,
            // password
            match options.password {
                None => Password::empty(),
                Some(ref p) => Password::from(p.as_str()),
            },
        )?;

        let files = options
            .files
            .clone()
            .map(|f| f.into_iter().collect::<HashSet<_>>());

        let _total_size: u64 = sz
            .archive()
            .files
            .iter()
            .filter(|e| e.has_stream())
            .map(|e| e.size())
            .sum();

        let mut uncompressed_size = 0;
        sz.for_each_entries(|entry, reader| {
            let mut buf = [0u8; 1024];
            let path = &options.destination.join(entry.name());

            if !options.overwrite && path.exists() {
                options.handle(ArchiveEvent::Skipped(
                    entry.name().to_string(),
                    SkipReason::AlreadyExists,
                ));
                return Ok(true);
            }

            if let Some(files) = &files {
                if !files.contains(&entry.name().to_string()) {
                    return Ok(true);
                }
            }

            if entry.is_directory() {
                options.handle(ArchiveEvent::Extracting(entry.name().to_string(), None));
                std::fs::create_dir_all(path)?;
                Ok(true)
            } else if entry.has_stream() {
                options.handle(ArchiveEvent::Extracting(
                    entry.name().to_string(),
                    Some(entry.size()),
                ));
                if let Some(p) = path.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(p)?;
                    }
                }

                let mut file = File::create(path)?;
                loop {
                    let read_size = reader.read(&mut buf)?;
                    if read_size == 0 {
                        break Ok(true);
                    }
                    file.write_all(&buf[..read_size])?;
                    uncompressed_size += read_size;
                }
            } else {
                options.handle(ArchiveEvent::Skipped(
                    entry.name().to_string(),
                    SkipReason::UnknownType,
                ));
                Ok(true)
            }
        })?;

        options.handle(ArchiveEvent::DoneExtracting(
            self.source.as_ref().to_string(),
            options.destination.to_string_lossy().to_string(),
        ));
        Ok(())
    }

    fn list(&self, options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError> {
        // eprintln!("list: options: {:?}", options);
        let mut reader = self.reader()?;

        let len = reader.len()?;
        let pw = options
            .password
            .clone()
            .map_or(Password::empty(), |p| Password::from(p.as_str()));

        let sz = SevenZReader::new(&mut reader, len, pw)?;

        let mut entries = Vec::<ArchiveFileEntity>::new();

        let mut reader = self.reader()?;

        for_each_entries(
            sz.archive(),
            Password::from(options.password.as_deref().unwrap_or_default()),
            &mut reader,
            |data, _reader| {
                let entry = data.entry;
                let estimated_compress_ratio =
                    match (data.folder_pack_size, data.folder_unpack_size) {
                        (Some(pack_size), Some(unpack_size)) => {
                            if pack_size == 0 {
                                None
                            } else {
                                Some(unpack_size as f64 / pack_size as f64)
                            }
                        }
                        _ => None,
                    };

                let estimated_compressed_size = match estimated_compress_ratio {
                    Some(ratio) => (entry.size() as f64 / ratio) as u64,
                    None => entry.size(),
                };

                let last_modified = entry.last_modified_date;
                let fstype = if entry.is_directory {
                    ArchiveFileEntityType::Directory
                } else if entry.has_stream {
                    ArchiveFileEntityType::File
                } else {
                    ArchiveFileEntityType::Unknown
                };
                let (size, compressed_size) = if entry.has_stream {
                    (Some(entry.size()), Some(estimated_compressed_size))
                } else {
                    (None, None)
                };
                let entity = ArchiveFileEntity {
                    name: entry.name.to_string(),
                    size,
                    compressed_size,
                    fstype,
                    last_modified: if entry.has_last_modified_date {
                        datetime_from_timestamp(last_modified.to_unix_time()).ok()
                    } else {
                        None
                    },
                    compression: data.compression.map(|c| c.name().to_string()),
                };

                entries.push(entity);

                Ok(true)
            },
        )?;

        Ok(entries)
    }

    fn create(options: CreateOptions) -> Result<CreateResult, ArchiveError> {
        #[cfg(not(feature = "lzma_codecs"))]
        {
            Err(ArchiveError::UnsupportedActionForArchiveType(
                "create".to_string(),
                crate::archive::ArchiveType::SevenZ,
            ))
        }

        #[cfg(feature = "lzma_codecs")]
        {
            let writer = File::create(&options.destination)?;
            let buf_writer = BufWriter::with_capacity(DEFAULT_BUF_SIZE, writer);

            let mut sz = SevenZWriter::new(buf_writer)?;

            let mut total_size: u64 = 0;
            let mut total_compressed_size: u64 = 0;

            for file in options.files {
                let metadata = std::fs::metadata(&file)?;
                eprintln!(
                    "Adding: {} ({})",
                    file.display(),
                    Byte::from(metadata.len()).get_appropriate_unit(byte_unit::UnitType::Both)
                );
                let res = sz.push_archive_entry::<File>(
                    SevenZArchiveEntry::from_path(
                        &file,
                        file.strip_prefix(&options.source)
                            .as_deref()
                            .unwrap_or(&file)
                            .to_string_lossy()
                            .to_string(),
                    ),
                    Some(File::open(file)?),
                )?;
                total_size += res.size();
                total_compressed_size += res.compressed_size;
            }

            sz.finish()?;
            eprintln!(
                "Done creating 7z archive: {} ({})",
                options.destination.display(),
                Byte::from(total_size).get_appropriate_unit(byte_unit::UnitType::Both)
            );
            Ok(CreateResult {
                path: options.destination,
                total_size,
                compressed_size: total_compressed_size,
            })
        }
    }

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError> {
        let mut reader = self.reader()?;
        let len = reader.len()?;
        let pw = Password::empty();
        let sz = SevenZReader::new(&mut reader, len, pw)?;

        let entries = self.list(ListOptions {
            password: None,
            event_handler: Box::new(SimpleLogger),
        })?;

        let size = entries.iter().filter_map(|f| f.size).sum();

        Ok(ArchiveMetadata {
            entries,
            total_size: size,
            compression: None,
            compressed_size: sz.archive().pack_sizes.iter().sum(),
            additional: None,
        })
    }

    fn open(&self, mut options: super::OpenOptions) -> Result<(), ArchiveError> {
        let path = options.path.to_string_lossy().to_string();
        let pw = match options.password {
            None => Password::empty(),
            Some(ref p) => Password::from(p.as_str()),
        };

        let mut reader = self.reader()?;

        let len = reader.len()?;

        let mut sz = SevenZReader::new(&mut reader, len, pw)?;

        let mut found = false;

        sz.for_each_entries(|entry, reader| {
            if entry.name() == path {
                std::io::copy(reader, &mut options.dest)?;
                found = true;
            } else {
                // still need to read the stream
                std::io::copy(reader, &mut std::io::sink())?;
            }
            Ok(!found)
        })?;

        if found {
            Ok(())
        } else {
            Err(ArchiveError::EntryNotFound(options.path))
        }
    }
}

struct SevenZForEachEntryData<'a> {
    entry: &'a SevenZArchiveEntry,
    folder_unpack_size: Option<u64>,
    folder_pack_size: Option<u64>,
    compression: Option<SevenZMethod>,
}

fn for_each_entries<
    F: FnMut(SevenZForEachEntryData, &mut dyn Read) -> Result<bool, sevenz_rust::Error>,
>(
    archive: &sevenz_rust::Archive,
    password: Password,
    source: &mut DataSource,
    mut each: F,
) -> Result<(), sevenz_rust::Error> {
    let folder_count = archive.folders.len();

    for folder_index in 0..folder_count {
        let forder_dec = BlockDecoder::new(folder_index, archive, password.as_slice(), source);
        let compression = archive
            .folders
            .get(folder_index)
            .and_then(|f| {
                f.ordered_coder_iter()
                    .next()
                    .map(|(_, c)| c.decompression_method_id())
            })
            .and_then(SevenZMethod::by_id);

        forder_dec.for_each_entries(&mut |entry, reader| {
            if !each(
                SevenZForEachEntryData {
                    entry,
                    folder_unpack_size: archive
                        .folders
                        .get(folder_index)
                        .map(|f| f.get_unpack_size()),
                    folder_pack_size: archive.pack_sizes.get(folder_index).copied(),
                    compression,
                },
                reader,
            )? {
                return Ok(false);
            }
            Ok(true)
        })?;
    }
    // decode empty files
    for file_index in 0..archive.files.len() {
        let folder_index = archive.stream_map.file_folder_index[file_index];
        if folder_index.is_none() {
            let file = &archive.files[file_index];
            let empty_reader: &mut dyn Read = &mut ([0u8; 0].as_slice());
            if !each(
                SevenZForEachEntryData {
                    entry: file,
                    folder_unpack_size: None,
                    folder_pack_size: None,
                    compression: None,
                },
                empty_reader,
            )? {
                return Ok(());
            }
        }
    }
    Ok(())
}
