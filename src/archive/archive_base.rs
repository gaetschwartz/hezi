use std::{
    fmt::Debug,
    fs::File,
    io::{Cursor, Error, ErrorKind, Read, Seek, SeekFrom, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use chrono::TimeZone;
use serde::{Deserialize, Serialize};

use crate::archive::codecs::ArchiveCodec;

use super::codecs::ArchiveCompression;

#[cfg(feature = "sevenz_archive")]
use super::sevenz_archive::SevenZArchive;

#[cfg(feature = "tar_archive")]
use super::tar_archive::TarArchive;

#[cfg(feature = "zip_archive")]
use super::zip_archive::ZipArchive;

#[cfg(feature = "iso_archive")]
use super::iso_archive::ISOArchive;

pub const DEFAULT_BUF_SIZE: usize = 32 * 1024;

pub trait Archived<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized;

    fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Self::of(DataSource::file(path)?)
    }

    fn from_bytes(bytes: &'a Vec<u8>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Self::of(DataSource::stream(bytes))
    }

    fn extract(&self, options: ExtractOptions) -> Result<(), ArchiveError>;

    fn list(&self, options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError>;

    fn create(options: CreateOptions) -> Result<CreateResult, ArchiveError>;

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError>;

    fn open(&'a self, options: OpenOptions) -> Result<(), ArchiveError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    pub total_size: u64,
    pub compressed_size: u64,
    pub compression: Option<ArchiveCompression>,
    pub entries: Vec<ArchiveFileEntity>,
    pub additional: Option<serde_json::Value>,
}

pub struct CreateResult {
    pub path: PathBuf,
    pub total_size: u64,
    pub compressed_size: u64,
}

pub enum Archive<'a> {
    #[cfg(feature = "zip_archive")]
    Zip(ZipArchive<'a>),
    #[cfg(feature = "tar_archive")]
    Tar(TarArchive<'a>),
    #[cfg(feature = "sevenz_archive")]
    SevenZ(SevenZArchive<'a>),
    #[cfg(feature = "iso_archive")]
    Iso(ISOArchive<'a>),
    _Unreachable(PhantomData<&'a ()>),
}

impl<'a> Archive<'a> {
    pub fn of(data: DataSource<'a>) -> Result<Self, ArchiveError> {
        match ArchiveType::try_from_datasource(data.clone())?.0 {
            #[cfg(feature = "zip_archive")]
            ArchiveType::Zip => Ok(Archive::Zip(ZipArchive { source: data })),
            #[cfg(feature = "tar_archive")]
            ArchiveType::Tar => Ok(Archive::Tar(TarArchive { source: data })),
            #[cfg(feature = "sevenz_archive")]
            ArchiveType::SevenZ => Ok(Archive::SevenZ(SevenZArchive { source: data })),
            #[cfg(feature = "iso_archive")]
            ArchiveType::Iso => Ok(Archive::Iso(ISOArchive { source: data })),
            ArchiveType::_Unreachable => unreachable!(),
        }
    }
}

impl<'a> Archived<'a> for Archive<'a> {
    fn of(source: DataSource<'a>) -> Result<Self, ArchiveError>
    where
        Self: Sized,
    {
        Self::of(source)
    }

    fn extract(&self, options: ExtractOptions) -> Result<(), ArchiveError> {
        match self {
            #[cfg(feature = "zip_archive")]
            Archive::Zip(a) => a.extract(options),
            #[cfg(feature = "tar_archive")]
            Archive::Tar(a) => a.extract(options),
            #[cfg(feature = "sevenz_archive")]
            Archive::SevenZ(a) => a.extract(options),
            #[cfg(feature = "iso_archive")]
            Archive::Iso(a) => a.extract(options),
            Archive::_Unreachable(_) => unreachable!(),
        }
    }

    fn list(&self, options: ListOptions) -> Result<Vec<ArchiveFileEntity>, ArchiveError> {
        match self {
            #[cfg(feature = "zip_archive")]
            Archive::Zip(a) => a.list(options),
            #[cfg(feature = "tar_archive")]
            Archive::Tar(a) => a.list(options),
            #[cfg(feature = "sevenz_archive")]
            Archive::SevenZ(a) => a.list(options),
            #[cfg(feature = "iso_archive")]
            Archive::Iso(a) => a.list(options),
            Archive::_Unreachable(_) => unreachable!(),
        }
    }

    fn create(options: CreateOptions) -> Result<CreateResult, ArchiveError> {
        let archive_type = ArchiveType::guess_from_filename(&options.destination)?.0;
        match archive_type {
            #[cfg(feature = "zip_archive")]
            ArchiveType::Zip => ZipArchive::create(options),
            #[cfg(feature = "tar_archive")]
            ArchiveType::Tar => TarArchive::create(options),
            #[cfg(feature = "sevenz_archive")]
            ArchiveType::SevenZ => SevenZArchive::create(options),
            #[cfg(feature = "iso_archive")]
            ArchiveType::Iso => ISOArchive::create(options),
            ArchiveType::_Unreachable => unreachable!(),
        }
    }

    fn metadata(&self) -> Result<ArchiveMetadata, ArchiveError> {
        match self {
            #[cfg(feature = "zip_archive")]
            Archive::Zip(a) => a.metadata(),
            #[cfg(feature = "tar_archive")]
            Archive::Tar(a) => a.metadata(),
            #[cfg(feature = "sevenz_archive")]
            Archive::SevenZ(a) => a.metadata(),
            #[cfg(feature = "iso_archive")]
            Archive::Iso(a) => a.metadata(),
            Archive::_Unreachable(_) => unreachable!(),
        }
    }

    fn open(&'a self, options: OpenOptions) -> Result<(), ArchiveError> {
        match self {
            #[cfg(feature = "zip_archive")]
            Archive::Zip(a) => a.open(options),
            #[cfg(feature = "tar_archive")]
            Archive::Tar(a) => a.open(options),
            #[cfg(feature = "sevenz_archive")]
            Archive::SevenZ(a) => a.open(options),
            #[cfg(feature = "iso_archive")]
            Archive::Iso(a) => a.open(options),
            Archive::_Unreachable(_) => unreachable!(),
        }
    }
}
#[derive(Debug)]
pub struct ExtractOptions<'a> {
    pub destination: PathBuf,
    pub password: Option<String>,
    pub files: Option<Vec<String>>,
    pub overwrite: bool,
    pub show_hidden: bool,
    pub event_handler: Box<dyn EventHandler + 'a>,
}

impl<'a> TryFrom<DataSource<'a>> for Archive<'a> {
    fn try_from(value: DataSource<'a>) -> Result<Self, Self::Error> {
        Archive::of(value)
    }

    type Error = ArchiveError;
}

#[derive(Debug)]
pub struct ListOptions<'a> {
    pub password: Option<String>,
    pub event_handler: Box<dyn EventHandler + 'a>,
}

#[derive(Debug)]
pub struct CreateOptions<'a> {
    pub destination: PathBuf,
    pub source: PathBuf,
    pub files: Vec<PathBuf>,
    pub password: Option<String>,
    pub archive_type: ArchiveType,
    pub archive_compression: Option<ArchiveCompression>,
    pub overwrite: bool,
    pub include_hidden: bool,
    pub event_handler: Box<dyn EventHandler + 'a>,
}

pub struct OpenOptions {
    pub path: PathBuf,
    pub password: Option<String>,
    pub dest: Box<dyn Write>,
}

impl Default for ExtractOptions<'_> {
    fn default() -> Self {
        Self {
            password: None,
            files: None,
            overwrite: false,
            show_hidden: true,
            destination: PathBuf::from("."),
            event_handler: Box::new(SimpleLogger),
        }
    }
}

impl Default for ListOptions<'_> {
    fn default() -> Self {
        Self {
            password: None,
            event_handler: Box::new(SimpleLogger),
        }
    }
}

impl<'a> EventHandler for ListOptions<'a> {
    fn handle(&self, event: ArchiveEvent) {
        self.event_handler.handle(event);
    }
}

impl<'a> EventHandler for ExtractOptions<'a> {
    fn handle(&self, event: ArchiveEvent) {
        self.event_handler.handle(event);
    }
}

impl<'a> EventHandler for CreateOptions<'a> {
    fn handle(&self, event: ArchiveEvent) {
        self.event_handler.handle(event);
    }
}

#[derive(Debug)]
pub struct SimpleLogger;

impl EventHandler for SimpleLogger {
    fn handle(&self, event: ArchiveEvent) {
        match event {
            ArchiveEvent::Extracting(name, size) => {
                if let Some(size) = size {
                    println!("Extracting {} ({})", name, size);
                } else {
                    println!("Extracting {}", name);
                }
            }
            ArchiveEvent::DoneExtracting(name, path) => {
                println!("Done extracting {} to {}", name, path);
            }
            ArchiveEvent::FailedToReadEntry(name, e) => {
                println!("Failed to read entry {}: {}", name, e);
            }
            ArchiveEvent::Created(name, fstype) => {
                println!("Created {}: {}", fstype, name);
            }
            ArchiveEvent::Skipped(name, reason) => match reason {
                SkipReason::Hidden => println!("Skipped hidden file {}", name),
                SkipReason::NotInFiles => println!("Skipped file {} not in files", name),
                SkipReason::AlreadyExists => println!("Skipped file {} already exists", name),
                SkipReason::UnknownType => println!("Skipped file {} with unknown type", name),
            },
            ArchiveEvent::Log(msg) => println!("{}", msg),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ArchiveType {
    #[cfg(feature = "zip_archive")]
    Zip,
    #[cfg(feature = "tar_archive")]
    Tar,
    #[cfg(feature = "sevenz_archive")]
    SevenZ,
    #[cfg(feature = "iso_archive")]
    Iso,
    _Unreachable,
}

impl ArchiveType {
    pub fn try_from_datasource(
        data: DataSource,
    ) -> Result<(ArchiveType, ArchiveCompression), ArchiveError> {
        let mut magic_bytes_0 = [0; 8];

        let mut reader = data.clone();

        reader.seek(SeekFrom::Start(0))?;
        reader.read_exact(&mut magic_bytes_0)?;
        // eprintln!("magic_bytes: {:04X?}", magic_bytes);

        if let Some(t) = match magic_bytes_0 {
            #[cfg(feature = "zip_archive")]
            [0x50, 0x4b, 0x03, 0x04, _, _, _, _]
            | [0x50, 0x4b, 0x05, 0x06, _, _, _, _]
            | [0x50, 0x4b, 0x07, 0x08, _, _, _, _] => Some(ArchiveType::Zip),
            #[cfg(feature = "sevenz_archive")]
            [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c, _, _] => Some(ArchiveType::SevenZ),
            _ => None,
        } {
            return Ok((t, ArchiveCompression::None));
        }

        #[cfg(feature = "tar_archive")]
        let mut magic_bytes_257 = [0; 8];
        #[cfg(feature = "tar_archive")]
        {
            reader.seek(SeekFrom::Start(257))?;
            reader.read_exact(&mut magic_bytes_257)?;
            const MAGIC_BYTES_TAR_1: [u8; 8] = [0x75, 0x73, 0x74, 0x61, 0x72, 0x00, 0x30, 0x30];
            const MAGIC_BYTES_TAR_2: [u8; 8] = [0x75, 0x73, 0x74, 0x61, 0x72, 0x20, 0x20, 0x00];

            if magic_bytes_257 == MAGIC_BYTES_TAR_1 || magic_bytes_257 == MAGIC_BYTES_TAR_2 {
                return Ok((ArchiveType::Tar, ArchiveCompression::None));
            }
            reader.seek(SeekFrom::Start(0))?;

            if let Ok(ref compression) =
                ArchiveCompression::try_from(MagicBytesAt::<8>(0, magic_bytes_0))
            {
                // eprintln!("compression: {:?}", compression);
                if let Ok(ref mut compression_reader) =
                    ArchiveCodec::get_reader(&mut reader, compression)
                {
                    // skip the first 257 bytes
                    std::io::copy(&mut compression_reader.take(257), &mut std::io::sink())?;
                    compression_reader.read_exact(&mut magic_bytes_257)?;
                    // eprintln!("magic_bytes_257: {:04X?}", magic_bytes_257);

                    if magic_bytes_257 == MAGIC_BYTES_TAR_1 || magic_bytes_257 == MAGIC_BYTES_TAR_2
                    {
                        return Ok((ArchiveType::Tar, compression.clone()));
                    }
                }
            }
        }

        // eprintln!("magic_bytes_257: {:04X?}", magic_bytes_257);

        // check for iso file

        #[cfg(feature = "iso_archive")]
        let mut magic_bytes_cd001_0x8001 = [0; 5];
        #[cfg(feature = "iso_archive")]
        let mut magic_bytes_cd001_0x8801 = [0; 5];
        #[cfg(feature = "iso_archive")]
        let mut magic_bytes_cd001_0x9001 = [0; 5];
        #[cfg(feature = "iso_archive")]
        {
            // check for iso file
            reader.seek(SeekFrom::Start(0x8001))?;
            reader.read_exact(&mut magic_bytes_cd001_0x8001)?;
            reader.seek(SeekFrom::Start(0x8801))?;
            reader.read_exact(&mut magic_bytes_cd001_0x8801)?;
            reader.seek(SeekFrom::Start(0x9001))?;
            reader.read_exact(&mut magic_bytes_cd001_0x9001)?;
            if magic_bytes_cd001_0x8001 == *b"CD001"
                && magic_bytes_cd001_0x8801 == *b"CD001"
                && magic_bytes_cd001_0x9001 == *b"CD001"
            {
                return Ok((ArchiveType::Iso, ArchiveCompression::None));
            }
        }

        Err(ArchiveError::UnknownArchiveType(MagicNumbers {
            #[cfg(feature = "zip_archive")]
            zip: MagicBytesAt(0, magic_bytes_0),
            #[cfg(feature = "tar_archive")]
            tar: MagicBytesAt(257, magic_bytes_257),
            #[cfg(feature = "iso_archive")]
            iso: (
                MagicBytesAt(0x8001, magic_bytes_cd001_0x8001),
                MagicBytesAt(0x8801, magic_bytes_cd001_0x8801),
                MagicBytesAt(0x9001, magic_bytes_cd001_0x9001),
            ),
        }))
    }

    pub fn guess_from_filename<R: AsRef<Path>>(
        path: R,
    ) -> Result<(ArchiveType, Option<ArchiveCompression>), ArchiveError> {
        let binding = path.as_ref().to_string_lossy();
        let split = binding.split('.').collect::<Vec<_>>();

        match (split.get(split.len() - 2), split[split.len() - 1]) {
            #[cfg(feature = "tar_archive")]
            (Some(&"tar"), "gz" | "gzip") | (_, "tgz") => {
                Ok((ArchiveType::Tar, Some(ArchiveCompression::Gzip)))
            }
            #[cfg(all(feature = "tar_archive", feature = "lzma_codecs"))]
            (Some(&"tar"), "xz") | (_, "txz") => {
                Ok((ArchiveType::Tar, Some(ArchiveCompression::Lzma)))
            }
            #[cfg(all(feature = "tar_archive", feature = "bzip2_codecs"))]
            (Some(&"tar"), "bz2") | (_, "tbz2") => {
                Ok((ArchiveType::Tar, Some(ArchiveCompression::Bzip2)))
            }
            #[cfg(all(feature = "tar_archive", feature = "zstd_codecs"))]
            (Some(&"tar"), "zst" | "zstd") | (_, "tzst") => {
                Ok((ArchiveType::Tar, Some(ArchiveCompression::Zstd)))
            }
            #[cfg(feature = "tar_archive")]
            (_, "tar") => Ok((ArchiveType::Tar, Some(ArchiveCompression::None))),
            #[cfg(feature = "zip_archive")]
            (_, "zip") => Ok((ArchiveType::Zip, None)),
            #[cfg(feature = "sevenz_archive")]
            (_, "7z" | "7zip") => Ok((ArchiveType::SevenZ, None)),
            #[cfg(feature = "iso_archive")]
            (_, "iso") => Ok((ArchiveType::Iso, None)),
            _ => Err(ArchiveError::UnknownFileExtension(
                path.as_ref().to_string_lossy().to_string(),
            )),
        }
    }
}

impl std::fmt::Display for ArchiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "zip_archive")]
            ArchiveType::Zip => write!(f, "zip"),
            #[cfg(feature = "tar_archive")]
            ArchiveType::Tar => write!(f, "tar"),
            #[cfg(feature = "sevenz_archive")]
            ArchiveType::SevenZ => write!(f, "7z"),
            #[cfg(feature = "iso_archive")]
            ArchiveType::Iso => write!(f, "iso"),
            ArchiveType::_Unreachable => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveFileEntity {
    pub(crate) name: String,
    pub(crate) size: Option<u64>,
    pub(crate) compressed_size: Option<u64>,
    pub(crate) last_modified: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(crate) compression: Option<String>,
    #[serde(rename = "type")]
    pub(crate) fstype: ArchiveFileEntityType,
}

impl ArchiveFileEntity {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn compressed_size(&self) -> Option<u64> {
        self.compressed_size
    }

    pub fn last_modified(&self) -> Option<chrono::DateTime<chrono::FixedOffset>> {
        self.last_modified
    }

    pub fn compression(&self) -> Option<&str> {
        self.compression.as_deref()
    }

    pub fn fstype(&self) -> ArchiveFileEntityType {
        self.fstype
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ArchiveFileEntityType {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "dir")]
    Directory,
    #[serde(rename = "symlink")]
    SymbolicLink,
    #[serde(rename = "unkwown")]
    Unknown,
}

impl std::fmt::Display for ArchiveFileEntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveFileEntityType::File => write!(f, "file"),
            ArchiveFileEntityType::Directory => write!(f, "dir"),
            ArchiveFileEntityType::SymbolicLink => write!(f, "symlink"),
            ArchiveFileEntityType::Unknown => write!(f, "unknown"),
        }
    }
}

impl TryFrom<&str> for ArchiveFileEntityType {
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "file" => Ok(ArchiveFileEntityType::File),
            "dir" => Ok(ArchiveFileEntityType::Directory),
            "symlink" => Ok(ArchiveFileEntityType::SymbolicLink),
            "unknown" => Ok(ArchiveFileEntityType::Unknown),
            _ => Err(()),
        }
    }

    type Error = ();
}

#[cfg(feature = "tar_archive")]
impl From<tar::EntryType> for ArchiveFileEntityType {
    fn from(t: tar::EntryType) -> Self {
        match t {
            tar::EntryType::Regular => ArchiveFileEntityType::File,
            tar::EntryType::Directory => ArchiveFileEntityType::Directory,
            tar::EntryType::Symlink => ArchiveFileEntityType::SymbolicLink,
            _ => ArchiveFileEntityType::Unknown,
        }
    }
}

pub fn datetime_from_timestamp(
    timestamp: i64,
) -> Result<chrono::DateTime<chrono::FixedOffset>, std::io::Error> {
    chrono::Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.fixed_offset())
        .ok_or(Error::new(
            ErrorKind::InvalidInput,
            "Invalid timestamp in tar archive",
        ))
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    Hidden,
    NotInFiles,
    AlreadyExists,
    UnknownType,
}

#[derive(Debug)]
pub enum ArchiveEvent {
    Extracting(String, Option<u64>),
    DoneExtracting(String, String),
    FailedToReadEntry(String, ArchiveError),
    Created(String, ArchiveFileEntityType),
    Skipped(String, SkipReason),
    Log(String),
}

pub trait EventHandler {
    fn handle(&self, event: ArchiveEvent);
}

impl<'a> Debug for dyn EventHandler + 'a {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EventHandler#{}", self as *const _ as *const u8 as usize)
    }
}

impl<T> EventHandler for Box<T>
where
    T: EventHandler,
{
    fn handle(&self, event: ArchiveEvent) {
        self.as_ref().handle(event);
    }
}

#[derive(Debug)]
pub enum ArchiveError {
    #[cfg(feature = "zip_archive")]
    Zip(zip::result::ZipError),
    #[cfg(feature = "zip_archive")]
    Password(zip::result::InvalidPassword),
    #[cfg(feature = "tar_archive")]
    Tar(std::io::Error),
    #[cfg(feature = "sevenz_archive")]
    SevenZ(sevenz_rust::Error),
    Io(std::io::Error),
    #[cfg(feature = "iso_archive")]
    Iso(cdfs::ISOError),
    #[cfg(feature = "lzma_codecs")]
    Lzma(lzma::LzmaError),
    UnknownArchiveType(MagicNumbers),
    UnknownFileExtension(String),
    InvalidDataSource(String),
    Finish(String, std::io::Error),
    UnsupportedCompression(ArchiveCompression),
    CompressionMethodRequired,
    UnsupportedActionForArchiveType(String, ArchiveType),
    Json(serde_json::Error),
    EntryNotFound(PathBuf),
}

#[derive(Debug)]
pub struct MagicNumbers {
    #[cfg(feature = "zip_archive")]
    zip: MagicBytesAt<8>,
    #[cfg(feature = "tar_archive")]
    tar: MagicBytesAt<8>,
    #[cfg(feature = "iso_archive")]
    iso: (
        MagicBytesAt<5, 's'>,
        MagicBytesAt<5, 's'>,
        MagicBytesAt<5, 's'>,
    ),
}

impl std::fmt::Display for MagicNumbers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v: Vec<String> = vec![
            #[cfg(feature = "zip_archive")]
            format!("zip at {}", self.zip),
            #[cfg(feature = "tar_archive")]
            format!("tar at {}", self.tar),
            #[cfg(feature = "iso_archive")]
            format!("iso at {}", self.iso.0),
            #[cfg(feature = "iso_archive")]
            format!("iso at {}", self.iso.1),
            #[cfg(feature = "iso_archive")]
            format!("iso at {}", self.iso.2),
        ];
        f.write_str(v.join(", ").as_str())?;
        Ok(())
    }
}

impl<const N: usize, const REPR: char> std::fmt::Display for MagicBytesAt<N, REPR> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if REPR == 'x' || REPR == 'X' {
            // print as string
            write!(f, "{:#08X}: '", self.0)?;
            for b in self.1.iter() {
                write!(f, "{}", *b as char)?;
            }
            write!(f, "'")?;
        } else {
            write!(f, "{:#08X}: [", self.0)?;
            let mut iter = self.1.iter();
            if let Some(b) = iter.next() {
                write!(f, "{:02X}", b)?;
            }
            for b in iter {
                write!(f, " {:02X}", b)?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

// implement std::error::Error and std::fmt::Display for ExtractError
impl std::error::Error for ArchiveError {}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "zip_archive")]
            ArchiveError::Zip(e) => write!(f, "ZipError: {}", e),
            #[cfg(feature = "zip_archive")]
            ArchiveError::Password(e) => write!(f, "PasswordError: {}", e),
            ArchiveError::Io(e) => write!(f, "{}", e),
            #[cfg(feature = "tar_archive")]
            ArchiveError::Tar(e) => write!(f, "TarError: {}", e),
            #[cfg(feature = "sevenz_archive")]
            ArchiveError::SevenZ(e) => write!(f, "SevenZError: {}", e),
            #[cfg(feature = "iso_archive")]
            ArchiveError::Iso(e) => write!(f, "ISOError: {}", e),
            #[cfg(feature = "lzma_codecs")]
            ArchiveError::Lzma(e) => write!(f, "LzmaError: {}", e),
            ArchiveError::UnknownArchiveType(n) => {
                write!(f, "Unknown archive type, magic numbers: {}", n)
            }
            ArchiveError::UnknownFileExtension(e) => write!(f, "Unknown file extension: {}", e),
            ArchiveError::InvalidDataSource(t) => {
                write!(f, "Invalid data source for the archive: {}", t)
            }
            ArchiveError::Finish(finisher, e) => write!(
                f,
                "FinishError: Failed to finish encoder {:?}: {:?}",
                finisher, e
            ),
            ArchiveError::UnsupportedCompression(c) => {
                write!(f, "Unsupported compression: {}", c)
            }
            ArchiveError::CompressionMethodRequired => {
                write!(f, "Compression method required for this type of archive.")
            }
            ArchiveError::UnsupportedActionForArchiveType(action, archive_type) => write!(
                f,
                "Action `{}` is unsupported for {} archives.",
                action, archive_type
            ),
            ArchiveError::Json(e) => write!(f, "JsonError: {}", e),
            ArchiveError::EntryNotFound(p) => write!(f, "Entry not found: {}", p.display()),
        }
    }
}

impl From<serde_json::Error> for ArchiveError {
    fn from(e: serde_json::Error) -> Self {
        ArchiveError::Json(e)
    }
}

#[cfg(feature = "zip_archive")]
impl From<zip::result::ZipError> for ArchiveError {
    fn from(e: zip::result::ZipError) -> Self {
        ArchiveError::Zip(e)
    }
}

#[cfg(feature = "zip_archive")]
impl From<zip::result::InvalidPassword> for ArchiveError {
    fn from(e: zip::result::InvalidPassword) -> Self {
        ArchiveError::Password(e)
    }
}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        ArchiveError::Io(e)
    }
}

pub trait AsTarArchiveResult<T> {
    fn into_tar_archive_result(self) -> Result<T, ArchiveError>;
}

impl<T> AsTarArchiveResult<T> for std::io::Result<T> {
    fn into_tar_archive_result(self) -> Result<T, ArchiveError> {
        self.map_err(ArchiveError::Tar)
    }
}

#[cfg(feature = "sevenz_archive")]
impl From<sevenz_rust::Error> for ArchiveError {
    fn from(e: sevenz_rust::Error) -> Self {
        ArchiveError::SevenZ(e)
    }
}

#[cfg(feature = "iso_archive")]
impl From<cdfs::ISOError> for ArchiveError {
    fn from(e: cdfs::ISOError) -> Self {
        ArchiveError::Iso(e)
    }
}

#[cfg(feature = "lzma_codecs")]
impl From<lzma::LzmaError> for ArchiveError {
    fn from(e: lzma::LzmaError) -> Self {
        ArchiveError::Lzma(e)
    }
}

#[derive(Debug)]
pub enum DataSource<'a> {
    File(Box<File>, String),
    Stream(Cursor<&'a Vec<u8>>),
}

impl std::fmt::Display for DataSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::File(_, path) => write!(f, "{}", path),
            // use the inner value pointer as a unique identifier
            DataSource::Stream(c) => {
                write!(f, " stream at {:?}", (c.get_ref() as *const _) as usize)
            }
        }
    }
}

impl<'a> DataSource<'a> {
    pub fn file<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let s = path.as_ref().to_string_lossy().to_string();
        let file = File::open(path)?;
        Ok(DataSource::File(Box::new(file), s))
    }

    pub fn stream(data: &'a Vec<u8>) -> Self {
        DataSource::Stream(Cursor::new(data))
    }

    pub fn try_clone(&self) -> Result<Self, std::io::Error> {
        match self {
            DataSource::File(_, path) => {
                Ok(DataSource::File(Box::new(File::open(path)?), path.clone()))
            }
            DataSource::Stream(val) => Ok(DataSource::Stream(Cursor::new(val.clone().get_ref()))),
        }
    }
}

pub trait Lengthed {
    fn len(&self) -> Result<u64, std::io::Error>;

    fn is_empty(&self) -> Result<bool, std::io::Error> {
        self.len().map(|l| l == 0)
    }
}

impl Lengthed for DataSource<'_> {
    fn len(&self) -> Result<u64, std::io::Error> {
        match self {
            DataSource::File(f, _) => f.metadata().map(|m| m.len()),
            DataSource::Stream(val) => Ok(val.get_ref().len() as u64),
        }
    }
}

pub trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

impl<'a> Read for DataSource<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            DataSource::File(file, _) => file.read(buf),
            DataSource::Stream(val) => val.read(buf),
        }
    }
}

impl<'a> Seek for DataSource<'a> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            DataSource::File(file, _) => file.seek(pos),
            DataSource::Stream(val) => val.seek(pos),
        }
    }
}

impl Clone for DataSource<'_> {
    fn clone(&self) -> Self {
        self.try_clone()
            .expect("Failed to clone DataSource, this should never happen")
    }
}

impl<'a> AsRef<DataSource<'a>> for DataSource<'a> {
    fn as_ref(&self) -> &DataSource<'a> {
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MagicBytesAt<const N: usize, const REPR: char = 'x'>(pub u64, pub [u8; N]);

pub type MagicBytesStr<const N: usize> = MagicBytesAt<N, 's'>;
pub type MagicBytesHex<const N: usize> = MagicBytesAt<N, 'x'>;

impl<const N: usize, const REPR: char> MagicBytesAt<N, REPR> {
    pub fn new(offset: u64, bytes: [u8; N]) -> Self {
        MagicBytesAt(offset, bytes)
    }
}

impl<const REPR: char> TryFrom<MagicBytesAt<8, REPR>> for ArchiveCompression {
    type Error = Error;

    fn try_from(magic: MagicBytesAt<8, REPR>) -> Result<Self, Self::Error> {
        match magic {
            MagicBytesAt(0, [0x1f, 0x8b, _, _, _, _, _, _]) => Ok(ArchiveCompression::Gzip),
            #[cfg(feature = "bzip2_codecs")]
            MagicBytesAt(0, [0x42, 0x5a, 0x68, _, _, _, _, _]) => Ok(ArchiveCompression::Bzip2),
            #[cfg(feature = "lzma_codecs")]
            MagicBytesAt(0, [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, _, _]) => {
                Ok(ArchiveCompression::Lzma)
            }
            #[cfg(feature = "zstd_codecs")]
            MagicBytesAt(0, [0x28, 0xb5, 0x2f, 0xfd, _, _, _, _]) => Ok(ArchiveCompression::Zstd),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("unknown magic bytes at {}: {:04x?}", magic.0, magic.1),
            )),
        }
    }
}

#[cfg(feature = "zip_archive")]
impl TryFrom<ArchiveCompression> for zip::CompressionMethod {
    fn try_from(value: ArchiveCompression) -> Result<Self, Self::Error> {
        match value {
            ArchiveCompression::None => Ok(zip::CompressionMethod::Stored),
            ArchiveCompression::Gzip => Err(Error::new(
                ErrorKind::InvalidInput,
                "Gzip compression is not supported for zip archives.",
            )),
            #[cfg(feature = "deflate_codecs")]
            ArchiveCompression::Deflate => Ok(zip::CompressionMethod::Deflated),
            #[cfg(feature = "bzip2_codecs")]
            ArchiveCompression::Bzip2 => Ok(zip::CompressionMethod::Bzip2),
            #[cfg(feature = "zstd_codecs")]
            ArchiveCompression::Zstd => Ok(zip::CompressionMethod::Zstd),
            #[cfg(feature = "aes_codecs")]
            ArchiveCompression::Aes => Ok(zip::CompressionMethod::Aes),
            #[cfg(feature = "lzma_codecs")]
            ArchiveCompression::Lzma => Err(Error::new(
                ErrorKind::InvalidInput,
                "Lzma compression is not supported for zip archives.",
            )),
            ArchiveCompression::Unknown(s) => Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Unknown compression method: {}", s),
            )),
        }
    }

    type Error = Error;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use std::io::{Read, Seek, SeekFrom};

    use super::*;

    #[test]
    fn test_archive_file_entity_type() {
        assert_eq!(
            ArchiveFileEntityType::try_from("file").unwrap(),
            ArchiveFileEntityType::File
        );
        assert_eq!(
            ArchiveFileEntityType::try_from("dir").unwrap(),
            ArchiveFileEntityType::Directory
        );
        assert_eq!(
            ArchiveFileEntityType::try_from("symlink").unwrap(),
            ArchiveFileEntityType::SymbolicLink
        );
        assert_eq!(
            ArchiveFileEntityType::try_from("unknown").unwrap(),
            ArchiveFileEntityType::Unknown
        );
    }

    #[test]
    fn test_seek() {
        let bfr = vec![1, 2, 3, 4, 5];
        let mut data = DataSource::stream(&bfr);
        let mut buf = [0; 2];
        data.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [1, 2]);

        data.seek(SeekFrom::Start(2)).unwrap();
        data.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [3, 4]);
    }

    #[test]
    fn test_seek_cloned() {
        let bfr = vec![1, 2, 3, 4, 5];
        let data = DataSource::stream(&bfr);
        let mut reader = data.clone();

        let mut buf = [0; 2];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [1, 2]);

        reader.seek(SeekFrom::Start(2)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [3, 4]);
    }

    #[test]

    fn archive_compression_from_magic_bytes() {
        let gzip = [0x1f, 0x8b, 0x08, 0x08, 0x5c, 0x5c, 0x5c, 0x5c];
        let lzma = [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x00];
        let bzip2 = [0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26];
        let zstd = [0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(
            ArchiveCompression::try_from(MagicBytesAt::<8>(0, gzip)).unwrap(),
            ArchiveCompression::Gzip
        );

        #[cfg(feature = "lzma_codecs")]
        assert_eq!(
            ArchiveCompression::try_from(MagicBytesAt::<8>(0, lzma)).unwrap(),
            ArchiveCompression::Lzma
        );

        #[cfg(feature = "bzip2_codecs")]
        assert_eq!(
            ArchiveCompression::try_from(MagicBytesAt::<8>(0, bzip2)).unwrap(),
            ArchiveCompression::Bzip2
        );

        #[cfg(feature = "zstd_codecs")]
        assert_eq!(
            ArchiveCompression::try_from(MagicBytesAt::<8>(0, zstd)).unwrap(),
            ArchiveCompression::Zstd
        );
    }

    #[test]
    fn archive_compression_from_datasource() -> Result<(), std::io::Error> {
        #[cfg(feature = "tar_archive")]
        {
            let gzip = DataSource::file("tests/fixtures/test1.tar.gz")?;
            assert_eq!(
                ArchiveType::try_from_datasource(gzip).unwrap(),
                (ArchiveType::Tar, ArchiveCompression::Gzip)
            );
            #[cfg(feature = "lzma_codecs")]
            {
                let lzma = DataSource::file("tests/fixtures/test1.tar.xz")?;
                assert_eq!(
                    ArchiveType::try_from_datasource(lzma).unwrap(),
                    (ArchiveType::Tar, ArchiveCompression::Lzma)
                );
            }

            #[cfg(feature = "bzip2_codecs")]
            {
                let bzip2 = DataSource::file("tests/fixtures/test1.tar.bz2")?;
                assert_eq!(
                    ArchiveType::try_from_datasource(bzip2).unwrap(),
                    (ArchiveType::Tar, ArchiveCompression::Bzip2)
                );
            }

            #[cfg(feature = "zstd_codecs")]
            {
                let zstd = DataSource::file("tests/fixtures/test1.tar.zst")?;
                assert_eq!(
                    ArchiveType::try_from_datasource(zstd).unwrap(),
                    (ArchiveType::Tar, ArchiveCompression::Zstd)
                );
            }

            let tar = DataSource::file("tests/fixtures/test1.tar")?;
            assert_eq!(
                ArchiveType::try_from_datasource(tar).unwrap(),
                (ArchiveType::Tar, ArchiveCompression::None)
            );
        }

        #[cfg(feature = "sevenz_archive")]
        {
            let sevenz = DataSource::file("tests/fixtures/test1.7z")?;
            assert_eq!(
                ArchiveType::try_from_datasource(sevenz).unwrap(),
                (ArchiveType::SevenZ, ArchiveCompression::None)
            );
        }

        #[cfg(feature = "zip_archive")]
        {
            let zip = DataSource::file("tests/fixtures/test1.zip")?;
            assert_eq!(
                ArchiveType::try_from_datasource(zip).unwrap(),
                (ArchiveType::Zip, ArchiveCompression::None)
            );
        }

        Ok(())
    }
}
