use nu_plugin::EvaluatedCall;
use nu_protocol::{CustomValue, LabeledError, Value};

use hezi::archive::{Archive, Archived, DataSource, ListOptions};

pub fn from_xx_archive<'a>(
    _name: &str,
    _call: &EvaluatedCall,
    input: &'a Value,
) -> Result<Value, LabeledError> {
    let span = input.span();

    // eprintln!("input type: {:?}", input.get_type());

    let datasource: DataSource<'a> = DataSource::try_from(input)
        .map_err(|_e| LabeledError::new("could not convert value to datasource"))?;

    // eprintln!("datasource: {}", datasource);

    let archive = Archive::of(datasource).map_err(|e| LabeledError::new(e.to_string()))?;

    let list = archive
        .list(ListOptions::default())
        .map_err(|e| LabeledError::new(e.to_string()))?;

    Ok(Value::List {
        vals: list
            .iter()
            .map(|f| f.to_base_value(span))
            .collect::<Result<Vec<_>, _>>()?,
        internal_span: span,
    })
}
