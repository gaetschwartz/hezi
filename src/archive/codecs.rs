// reader class that implements Read + Seek and takes a file as input
// it detects the compression type and returns the appropriate reader
// uses flat2 to decompress gzip and xz
// uses bzip2 to decompress bzip2
// uses rust-lzma to decompress lzma
// uses zstd to decompress zstd

use std::io::{BufReader, Error, Read, Write};

#[cfg(feature = "lzma_codecs")]
use lzma::{LzmaReader, LzmaWriter};
#[cfg(feature = "sevenz_archive")]
use sevenz_rust::SevenZMethod;
use strum::EnumIter;

use crate::archive::{ArchiveError, ReadSeek};

pub struct ArchiveCodec;

impl ArchiveCodec {
    pub(crate) fn get_reader<'a, R: ReadSeek + 'a>(
        inner: R,
        compression: &ArchiveCompression,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        match compression {
            ArchiveCompression::None => {
                let reader = std::io::BufReader::new(inner);
                Ok(Box::new(reader))
            }
            ArchiveCompression::Gzip => Ok(Box::new(flate2::bufread::GzDecoder::new(
                BufReader::new(inner),
            ))),
            #[cfg(feature = "deflate_codecs")]
            ArchiveCompression::Deflate => Ok(Box::new(flate2::bufread::ZlibDecoder::new(
                BufReader::new(inner),
            ))),
            #[cfg(feature = "bzip2_codecs")]
            ArchiveCompression::Bzip2 => Ok(Box::new(bzip2::bufread::BzDecoder::new(
                BufReader::new(inner),
            ))),
            #[cfg(feature = "lzma_codecs")]
            ArchiveCompression::Lzma => Ok(Box::new(LzmaReader::new_decompressor(inner)?)),
            #[cfg(feature = "zstd_codecs")]
            ArchiveCompression::Zstd => Ok(Box::new(zstd::Decoder::new(inner)?)),
            #[cfg(feature = "aes_codecs")]
            ArchiveCompression::Aes => Err(ArchiveError::UnsupportedCompression(
                ArchiveCompression::Aes,
            )),

            ArchiveCompression::Unknown(s) => Err(ArchiveError::UnsupportedCompression(
                ArchiveCompression::Unknown(s.to_string()),
            )),
        }
    }

    pub(crate) fn get_writer<'w, R: Write + 'w>(
        tar_compression: &ArchiveCompression,
        writer: R,
    ) -> Result<Box<dyn FinishableWrite + 'w>, ArchiveError> {
        let writer: Box<dyn FinishableWrite + 'w> = match tar_compression {
            ArchiveCompression::None => Box::new(NoOpFinishableWrite(writer)),
            ArchiveCompression::Gzip => Box::new(flate2::write::GzEncoder::new(
                writer,
                flate2::Compression::default(),
            )),
            #[cfg(feature = "deflate_codecs")]
            ArchiveCompression::Deflate => Box::new(flate2::write::ZlibEncoder::new(
                writer,
                flate2::Compression::default(),
            )),
            #[cfg(feature = "bzip2_codecs")]
            ArchiveCompression::Bzip2 => Box::new(bzip2::write::BzEncoder::new(
                writer,
                bzip2::Compression::default(),
            )),
            #[cfg(feature = "lzma_codecs")]
            ArchiveCompression::Lzma => Box::new(LzmaWriter::new_compressor(writer, 6)?),
            #[cfg(feature = "zstd_codecs")]
            ArchiveCompression::Zstd => {
                let mut enc = zstd::Encoder::new(writer, 0)?;

                #[cfg(feature = "multithreading")]
                {
                    _ = enc.multithread(
                        std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
                    );
                }
                Box::new(enc)
            }
            #[cfg(feature = "aes_codecs")]
            ArchiveCompression::Aes => {
                return Err(ArchiveError::UnsupportedCompression(
                    ArchiveCompression::Aes,
                ))
            }
            ArchiveCompression::Unknown(s) => {
                return Err(ArchiveError::UnsupportedCompression(
                    ArchiveCompression::Unknown(s.to_string()),
                ))
            }
        };

        Ok(writer)
    }
}

#[derive(
    Debug, Clone, PartialEq, EnumIter, serde::Serialize, serde::Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveCompression {
    Gzip,
    #[cfg(feature = "bzip2_codecs")]
    Bzip2,
    #[cfg(feature = "lzma_codecs")]
    Lzma,
    #[cfg(feature = "zstd_codecs")]
    Zstd,
    #[cfg(feature = "aes_codecs")]
    Aes,
    #[cfg(feature = "deflate_codecs")]
    Deflate,
    // skip value enum
    #[clap(skip)]
    Unknown(String),
    None,
}

impl ArchiveCompression {
    pub fn valid_level_range(&self) -> Option<std::ops::RangeInclusive<i32>> {
        match self {
            ArchiveCompression::Gzip => Some(0..=9),
            #[cfg(feature = "bzip2_codecs")]
            ArchiveCompression::Bzip2 => Some(0..=9),
            #[cfg(feature = "lzma_codecs")]
            ArchiveCompression::Lzma => Some(0..=9),
            #[cfg(feature = "zstd_codecs")]
            ArchiveCompression::Zstd => Some(1..=19),
            #[cfg(feature = "aes_codecs")]
            ArchiveCompression::Aes => None,
            #[cfg(feature = "deflate_codecs")]
            ArchiveCompression::Deflate => Some(0..=9),
            ArchiveCompression::Unknown(_) => None,
            ArchiveCompression::None => None,
        }
    }
}

#[cfg(feature = "sevenz_archive")]
impl From<SevenZMethod> for ArchiveCompression {
    fn from(value: SevenZMethod) -> Self {
        match value {
            #[cfg(feature = "lzma_codecs")]
            SevenZMethod::LZMA | SevenZMethod::LZMA2 => ArchiveCompression::Lzma,
            #[cfg(feature = "zstd_codecs")]
            SevenZMethod::ZSTD => ArchiveCompression::Zstd,
            #[cfg(feature = "deflate_codecs")]
            SevenZMethod::DEFLATE | SevenZMethod::DEFLATE64 => ArchiveCompression::Deflate,
            #[cfg(feature = "bzip2_codecs")]
            SevenZMethod::BZIP2 => ArchiveCompression::Bzip2,
            #[cfg(feature = "aes_codecs")]
            SevenZMethod::AES256SHA256 => ArchiveCompression::Aes,
            _ => ArchiveCompression::Unknown(value.name().to_string()),
        }
    }
}

impl std::fmt::Display for ArchiveCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveCompression::Gzip => write!(f, "gzip"),
            #[cfg(feature = "bzip2_codecs")]
            ArchiveCompression::Bzip2 => write!(f, "bzip2"),
            #[cfg(feature = "lzma_codecs")]
            ArchiveCompression::Lzma => write!(f, "lzma"),
            #[cfg(feature = "zstd_codecs")]
            ArchiveCompression::Zstd => write!(f, "zstd"),
            #[cfg(feature = "aes_codecs")]
            ArchiveCompression::Aes => write!(f, "aes"),
            #[cfg(feature = "deflate_codecs")]
            ArchiveCompression::Deflate => write!(f, "deflate"),
            ArchiveCompression::None => write!(f, "none"),
            ArchiveCompression::Unknown(s) => write!(f, "unknown ({})", s),
        }
    }
}

#[derive(Debug)]
pub(crate) struct FinishError<E> {
    inner: E,
    name: String,
}

impl<E> FinishError<E> {
    pub(crate) fn new<S: Into<String>>(name: S, inner: E) -> Self {
        Self {
            name: name.into(),
            inner,
        }
    }
}

impl From<FinishError<Error>> for ArchiveError {
    fn from(val: FinishError<Error>) -> Self {
        ArchiveError::Finish(val.name.to_string(), val.inner)
    }
}

pub(crate) trait FinishableWrite: Write {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>>;
}

impl<T: FinishableWrite> FinishableWrite for Box<T> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        self.as_mut().finish_writer()
    }
}

#[cfg(feature = "bzip2_codecs")]
impl<W: Write> FinishableWrite for bzip2::write::BzEncoder<W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        bzip2::write::BzEncoder::try_finish(self).map_err(|e| FinishError::new("BzEncoder", e))
    }
}

impl<W: Write> FinishableWrite for flate2::write::GzEncoder<W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        flate2::write::GzEncoder::try_finish(self).map_err(|e| FinishError::new("GzEncoder", e))
    }
}

impl<W: Write> FinishableWrite for flate2::write::ZlibEncoder<W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        flate2::write::ZlibEncoder::try_finish(self).map_err(|e| FinishError::new("ZlibEncoder", e))
    }
}

#[cfg(feature = "lzma_codecs")]
impl<W: Write> FinishableWrite for LzmaWriter<W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        Ok(())
    }
}

#[cfg(feature = "zstd_codecs")]
impl<W: Write> FinishableWrite for zstd::Encoder<'_, W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        self.do_finish()
            .map_err(|e| FinishError::new("zstd::Encoder", e))
    }
}

pub(crate) struct NoOpFinishableWrite<W: Write>(pub(crate) W);

impl<W: Write> Write for NoOpFinishableWrite<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.0.flush()
    }
}

impl<W: Write> FinishableWrite for NoOpFinishableWrite<W> {
    fn finish_writer(&mut self) -> Result<(), FinishError<Error>> {
        Ok(())
    }
}

impl AsRef<ArchiveCompression> for ArchiveCompression {
    fn as_ref(&self) -> &ArchiveCompression {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_compression_to_string() {
        assert_eq!(ArchiveCompression::Gzip.to_string(), "gzip");
        assert_eq!(ArchiveCompression::Bzip2.to_string(), "bzip2");
        assert_eq!(ArchiveCompression::Lzma.to_string(), "lzma");
        assert_eq!(ArchiveCompression::Zstd.to_string(), "zstd");
        assert_eq!(ArchiveCompression::Aes.to_string(), "aes");
        assert_eq!(ArchiveCompression::Deflate.to_string(), "deflate");
        assert_eq!(ArchiveCompression::None.to_string(), "none");
        assert_eq!(
            ArchiveCompression::Unknown("foo".to_string()).to_string(),
            "unknown (foo)"
        );
    }
}
