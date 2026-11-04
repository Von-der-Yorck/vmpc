//! Structured text data: JSON ⇄ YAML ⇄ TOML ⇄ CSV/TSV, through a JSON value as the
//! common model. Tables (CSV/TSV) become arrays of objects and back; nested values in
//! a table cell are written as JSON.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::catalog::DataFormat;
use crate::util;

pub fn parse(text: &str, format: DataFormat) -> Result<Value, String> {
    match format {
        DataFormat::Json => serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}")),
        DataFormat::Yaml => serde_yaml::from_str(text).map_err(|e| format!("Invalid YAML: {e}")),
        DataFormat::Toml => toml::from_str::<toml::Value>(text)
            .map_err(|e| format!("Invalid TOML: {e}"))
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
        DataFormat::Csv => parse_table(text, b','),
        DataFormat::Tsv => parse_table(text, b'\t'),
    }
}

fn parse_table(text: &str, delimiter: u8) -> Result<Value, String> {
    let mut reader = csv::ReaderBuilder::new().delimiter(delimiter).flexible(true).from_reader(text.as_bytes());
    let headers: Vec<String> = reader.headers().map_err(|e| format!("Invalid table: {e}"))?.iter().map(str::to_string).collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("Invalid table: {e}"))?;
        let mut obj = Map::new();
        for (i, cell) in record.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("column{}", i + 1));
            obj.insert(key, cell_value(cell));
        }
        rows.push(Value::Object(obj));
    }
    Ok(Value::Array(rows))
}

/// Numbers and booleans in a table become real numbers/booleans.
fn cell_value(cell: &str) -> Value {
    let t = cell.trim();
    if let Ok(i) = t.parse::<i64>() {
        return Value::from(i);
    }
    if let Ok(f) = t.parse::<f64>() {
        if f.is_finite() && t.contains('.') {
            return Value::from(f);
        }
    }
    match t {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(cell.to_string()),
    }
}

pub fn write(value: &Value, format: DataFormat) -> Result<String, String> {
    match format {
        DataFormat::Json => serde_json::to_string_pretty(value).map(|s| s + "\n").map_err(|e| e.to_string()),
        DataFormat::Yaml => serde_yaml::to_string(value).map_err(|e| e.to_string()),
        DataFormat::Toml => {
            // TOML needs a table at the top and has no null.
            let table = match strip_nulls(value) {
                Value::Object(m) => Value::Object(m),
                other => {
                    let mut m = Map::new();
                    m.insert("items".into(), other);
                    Value::Object(m)
                }
            };
            toml::to_string_pretty(&table).map_err(|e| format!("Can't express this as TOML: {e}"))
        }
        DataFormat::Csv => write_table(value, b','),
        DataFormat::Tsv => write_table(value, b'\t'),
    }
}

fn strip_nulls(value: &Value) -> Value {
    match value {
        Value::Object(m) => Value::Object(m.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), strip_nulls(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().filter(|v| !v.is_null()).map(strip_nulls).collect()),
        v => v.clone(),
    }
}

fn write_table(value: &Value, delimiter: u8) -> Result<String, String> {
    // Rows: an array of objects; a single object (or object of arrays) becomes rows too.
    let rows: Vec<Value> = match value {
        Value::Array(a) => a.clone(),
        Value::Object(m) if m.values().all(Value::is_array) && m.len() == 1 => m.values().next().unwrap().as_array().unwrap().clone(),
        other => vec![other.clone()],
    };
    let mut headers: Vec<String> = Vec::new();
    for row in &rows {
        match row {
            Value::Object(m) => {
                for k in m.keys() {
                    if !headers.contains(k) {
                        headers.push(k.clone());
                    }
                }
            }
            _ if !headers.contains(&"value".to_string()) => headers.push("value".into()),
            _ => {}
        }
    }
    let cell = |v: Option<&Value>| match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v @ (Value::Array(_) | Value::Object(_))) => v.to_string(),
        Some(v) => v.to_string(),
    };
    let mut writer = csv::WriterBuilder::new().delimiter(delimiter).from_writer(Vec::new());
    writer.write_record(&headers).map_err(|e| e.to_string())?;
    for row in &rows {
        let record: Vec<String> = headers
            .iter()
            .map(|h| match row {
                Value::Object(m) => cell(m.get(h)),
                v if h == "value" => cell(Some(v)),
                _ => String::new(),
            })
            .collect();
        writer.write_record(&record).map_err(|e| e.to_string())?;
    }
    String::from_utf8(writer.into_inner().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Converts `source` (already read as `text`) and writes the result next to it (or into `dir`).
pub fn convert_file(source: &Path, text: &str, from: DataFormat, to: DataFormat, dir: Option<&Path>) -> Result<PathBuf, String> {
    let value = parse(text, from)?;
    let out_text = write(&value, to)?;
    let parent = dir.map(PathBuf::from).or_else(|| source.parent().map(PathBuf::from)).unwrap_or_else(|| util::default_output_dir("Documents"));
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "data".into());
    let output = util::unique_path(&parent, &stem, to.ext());
    std::fs::write(&output, out_text).map_err(|e| e.to_string())?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[{"name":"Ada","age":36,"admin":true},{"name":"Linus","age":54,"admin":false}]"#;

    #[test]
    fn round_trips_through_every_format() {
        let value = parse(JSON, DataFormat::Json).unwrap();
        for f in DataFormat::ALL {
            let text = write(&value, f).unwrap();
            let back = parse(&text, f).unwrap();
            let back = match (f, back) {
                // TOML wraps a top-level array in a table.
                (DataFormat::Toml, Value::Object(mut m)) => m.remove("items").unwrap(),
                (_, v) => v,
            };
            assert_eq!(back, value, "{f:?} round trip:\n{text}");
        }
    }

    #[test]
    fn csv_with_quotes_and_missing_cells() {
        let csv = "city,note\n\"Berlin, DE\",\"said \"\"hi\"\"\"\nParis\n";
        let v = parse(csv, DataFormat::Csv).unwrap();
        assert_eq!(v[0]["city"], "Berlin, DE");
        assert_eq!(v[0]["note"], "said \"hi\"");
        assert!(v[1].get("note").is_none());
        let yaml = write(&v, DataFormat::Yaml).unwrap();
        assert!(yaml.contains("Berlin, DE"));
    }

    #[test]
    fn toml_config_to_json_and_yaml() {
        let toml = "title = \"VMPC\"\n[engine]\nhwaccel = true\nthreads = 4\n";
        let v = parse(toml, DataFormat::Toml).unwrap();
        assert_eq!(v["engine"]["threads"], 4);
        let json = write(&v, DataFormat::Json).unwrap();
        assert_eq!(parse(&json, DataFormat::Json).unwrap(), v);
        assert!(parse("not = [valid", DataFormat::Toml).is_err());
    }
}
