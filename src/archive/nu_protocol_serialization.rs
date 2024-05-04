use nu_protocol::{CustomValue, FromValue, Record, ShellError, Span, Value};
use num::traits::AsPrimitive;
use strum::IntoEnumIterator;

use crate::archive::{ArchiveFileEntity, ArchiveMetadata};

use super::{ArchiveCompression, ArchiveError, DataSource};

impl CustomValue for ArchiveMetadata {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom(Box::new(self.clone()), span)
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        let json_value =
            serde_json::to_value(self.clone()).map_err(|e| ShellError::CantConvert {
                from_type: "ArchiveMetadata".to_string(),
                to_type: "JsonValue".to_string(),
                span,
                help: Some(e.to_string()),
            })?;

        let nu_value =
            json_value_to_nu_value(json_value, span).map_err(|e| e.into_shell_error(span))?;

        Ok(nu_value)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        "ArchiveMetadata"
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!()
    }

    #[doc = r" The friendly type name to show for the custom value, e.g. in `describe` and in error"]
    #[doc = r" messages. This does not have to be the same as the name of the struct or enum, but"]
    #[doc = r" conventionally often is."]
    fn type_name(&self) -> String {
        "ArchiveMetadata".to_string()
    }

    #[doc = r" Any representation used to downcast object to its original type (mutable reference)"]
    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub enum JsonToNushellValueError {
    InvalidInt(serde_json::Number),
}

impl JsonToNushellValueError {
    fn into_shell_error(self, span: Span) -> ShellError {
        match self {
            JsonToNushellValueError::InvalidInt(n) => ShellError::CantConvert {
                from_type: "JsonValue".to_string(),
                to_type: "Int".to_string(),
                span,
                help: Some(format!("Invalid integer value: {}", n)),
            },
        }
    }
}

fn json_value_to_nu_value(
    value: serde_json::Value,
    span: Span,
) -> Result<Value, JsonToNushellValueError> {
    match value {
        serde_json::Value::Null => Ok(Value::nothing(span)),
        serde_json::Value::Bool(b) => Ok(Value::bool(b, span)),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Ok(Value::int(i, span)),
            None => Err(JsonToNushellValueError::InvalidInt(n)),
        },
        serde_json::Value::String(s) => Ok(Value::string(s, span)),
        serde_json::Value::Array(v) => Ok(Value::list(
            v.iter()
                .map(|e| json_value_to_nu_value(e.clone(), span))
                .collect::<Result<_, _>>()?,
            span,
        )),
        serde_json::Value::Object(m) => {
            let mut record = Record::new();
            for (k, v) in m {
                record.insert(k, json_value_to_nu_value(v, span)?);
            }
            Ok(Value::record(record, span))
        }
    }
}

impl CustomValue for ArchiveFileEntity {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom(Box::new(self.clone()), span)
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        Ok(Value::record(
            Record::from_raw_cols_vals(
                vec![
                    "name".to_string(),
                    "size".to_string(),
                    "compressed_size".to_string(),
                    "type".to_string(),
                    "last_modified".to_string(),
                    "compression".to_string(),
                ],
                vec![
                    Value::String {
                        val: self.name.clone(),
                        internal_span: span,
                    },
                    self.size.to_filesize_value(span),
                    self.compressed_size.to_filesize_value(span),
                    Value::String {
                        val: self.fstype.to_string(),
                        internal_span: span,
                    },
                    self.last_modified.to_date_value(span),
                    self.compression.to_string_value(span),
                ],
                span,
                span,
            )?,
            span,
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[doc(hidden)]
    fn typetag_name(&self) -> &'static str {
        "ArchiveFileEntity"
    }

    fn type_name(&self) -> String {
        "ArchiveFileEntity".to_string()
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    #[doc(hidden)]
    fn typetag_deserialize(&self) {
        unimplemented!()
    }
}

pub trait ToFilesize {
    fn to_filesize_value(&self, span: Span) -> Value;
}

pub trait ToStringOrNothingValue {
    fn to_string_value(&self, span: Span) -> Value;
}

pub trait ToDateOrNothingValue {
    fn to_date_value(&self, span: Span) -> Value;
}

pub trait ToValueOrNothing<T> {
    fn to_value_or_nothing<F: FnOnce(&T) -> Value>(&self, span: Span, f: F) -> Value;
}

impl<T> ToValueOrNothing<T> for Option<T> {
    fn to_value_or_nothing<F: FnOnce(&T) -> Value>(&self, span: Span, f: F) -> Value {
        match self {
            Some(v) => f(v),
            None => Value::nothing(span),
        }
    }
}

impl<S: ToString> ToStringOrNothingValue for Option<S> {
    fn to_string_value(&self, span: Span) -> Value {
        match self {
            Some(s) => Value::string(s.to_string(), span),
            None => Value::nothing(span),
        }
    }
}

impl ToDateOrNothingValue for Option<chrono::DateTime<chrono::FixedOffset>> {
    fn to_date_value(&self, span: Span) -> Value {
        match self {
            Some(ref dt) => Value::date(*dt, span),
            None => Value::nothing(span),
        }
    }
}

impl<T: AsPrimitive<i64>> ToFilesize for Option<T> {
    fn to_filesize_value(&self, span: Span) -> Value {
        match self {
            Some(ref size) => Value::filesize((*size).as_(), span),
            None => Value::nothing(span),
        }
    }
}

impl<'a> TryFrom<&'a Value> for DataSource<'a> {
    type Error = ArchiveError;

    fn try_from(value: &'a Value) -> Result<DataSource<'a>, Self::Error> {
        match value {
            Value::Binary { val, .. } => Ok(DataSource::stream(val)),
            v => Err(ArchiveError::InvalidDataSource(v.get_type().to_string())),
        }
    }
}

impl FromValue for ArchiveCompression {
    fn from_value(value: Value) -> Result<Self, nu_protocol::ShellError> {
        match value {
            Value::String { ref val, .. } => match val.as_str().to_lowercase().as_str() {
                "gzip" => Ok(ArchiveCompression::Gzip),
                #[cfg(feature = "bzip2_codecs")]
                "bzip2" => Ok(ArchiveCompression::Bzip2),
                #[cfg(feature = "lzma_codecs")]
                "lzma" | "xz" => Ok(ArchiveCompression::Lzma),
                #[cfg(feature = "zstd_codecs")]
                "zstd" => Ok(ArchiveCompression::Zstd),
                #[cfg(feature = "aes_codecs")]
                "aes" => Ok(ArchiveCompression::Aes),
                "none" | "false" => Ok(ArchiveCompression::None),
                #[cfg(feature = "deflate_codecs")]
                "deflate" | "deflated" => Ok(ArchiveCompression::Deflate),
                _ => {
                    let lower = val.to_lowercase();
                    let closest = ArchiveCompression::iter()
                        .map(|c| {
                            (
                                nu_protocol::levenshtein_distance(
                                    lower.as_str(),
                                    c.to_string().to_lowercase().as_str(),
                                ),
                                c,
                            )
                        })
                        .reduce(|a, b| if a.0 < b.0 { a } else { b });
                    if let Some((0..=3, c)) = closest {
                        Err(nu_protocol::ShellError::DidYouMean {
                            suggestion: c.to_string(),
                            span: value.span(),
                        })
                    } else {
                        Err(nu_protocol::ShellError::CantConvert {
                            from_type: format!("\"{}\"", val),
                            to_type: "ArchiveCompression".to_string(),
                            span: value.span(),
                            help: None,
                        })
                    }
                }
            },
            _ => Err(nu_protocol::ShellError::CantConvert {
                from_type: value.get_type().to_string(),
                to_type: "ArchiveCompression".to_string(),
                span: value.span(),
                help: None,
            }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::archive::{ArchiveCompression, ArchiveFileEntityType, ArchiveMetadata};
    use nu_protocol::{record, Value};
    use serde_json::json;

    #[test]
    fn test_json_value_to_nu_value() {
        let json = serde_json::json!({
            "a": 1,
            "b": "2",
            "c": [1, 2, 3],
            "d": {
                "e": "f"
            },
            "g": null,
            "h": true,
        });

        let nu_value = json_value_to_nu_value(json, Span::unknown()).unwrap();

        assert_eq!(
            nu_value,
            Value::record(
                Record::from_raw_cols_vals(
                    vec![
                        "a".to_string(),
                        "b".to_string(),
                        "c".to_string(),
                        "d".to_string(),
                        "g".to_string(),
                        "h".to_string(),
                    ],
                    vec![
                        Value::int(1, Span::unknown()),
                        Value::string("2", Span::unknown()),
                        Value::list(
                            vec![
                                Value::int(1, Span::unknown()),
                                Value::int(2, Span::unknown()),
                                Value::int(3, Span::unknown()),
                            ],
                            Span::unknown()
                        ),
                        Value::record(
                            Record::from_iter(vec![(
                                "e".to_string(),
                                Value::string("f", Span::unknown())
                            )]),
                            Span::unknown()
                        ),
                        Value::nothing(Span::unknown()),
                        Value::bool(true, Span::unknown()),
                    ],
                    Span::unknown(),
                    Span::unknown()
                )
                .unwrap(),
                Span::unknown()
            )
        );
    }

    #[test]
    fn test_archive_metadata_to_value() {
        let metadata = ArchiveMetadata {
            compressed_size: 360,
            compression: Some(ArchiveCompression::Zstd),
            total_size: 420,
            entries: vec![ArchiveFileEntity {
                name: "test".to_string(),
                size: Some(100),
                compressed_size: Some(69),
                last_modified: Some(
                    chrono::DateTime::parse_from_rfc3339("2021-01-01T00:00:00Z").unwrap(),
                ),
                compression: Some(ArchiveCompression::Zstd.to_string()),
                fstype: ArchiveFileEntityType::File,
            }],
            additional: Some(json!(
                {
                    "details": "test",
                    "attributes": {
                        "test": "test"
                    },
                    "flags": ["hidden", "readonly"],
                }
            )),
        };

        let value = metadata.to_base_value(Span::unknown()).unwrap();

        assert_eq!(
            value,
            Value::record(
                Record::from_raw_cols_vals(
                    vec![
                        "total_size".to_string(),
                        "compressed_size".to_string(),
                        "compression".to_string(),
                        "entries".to_string(),
                        "additional".to_string(),
                    ],
                    vec![
                        Value::int(420, Span::unknown()),
                        Value::int(360, Span::unknown()),
                        Value::string("zstd", Span::unknown()),
                        Value::list(
                            vec![Value::record(
                                Record::from_raw_cols_vals(
                                    vec![
                                        "name".to_string(),
                                        "size".to_string(),
                                        "compressed_size".to_string(),
                                        "last_modified".to_string(),
                                        "compression".to_string(),
                                        "type".to_string(),
                                    ],
                                    vec![
                                        Value::string("test", Span::unknown()),
                                        Value::int(100, Span::unknown()),
                                        Value::int(69, Span::unknown()),
                                        Value::string("2021-01-01T00:00:00Z", Span::unknown()),
                                        Value::string("zstd", Span::unknown()),
                                        Value::string("file", Span::unknown()),
                                    ],
                                    Span::unknown(),
                                    Span::unknown()
                                )
                                .unwrap(),
                                Span::unknown()
                            )],
                            Span::unknown()
                        ),
                        Value::record(
                            record! {
                                "details" => Value::string("test", Span::unknown()),
                                "attributes" => Value::record(record! {
                                    "test" => Value::string("test", Span::unknown()),
                                }, Span::unknown()),
                                "flags" => Value::list(vec![
                                    Value::string("hidden", Span::unknown()),
                                    Value::string("readonly", Span::unknown()),
                                ], Span::unknown()),
                            },
                            Span::unknown()
                        ),
                    ],
                    Span::unknown(),
                    Span::unknown()
                )
                .unwrap(),
                Span::unknown()
            )
        );
    }
}
