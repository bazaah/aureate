use {
    serde::Serialize,
    serde_json::{map::Map as JMap, value::Value as JsonValue},
    serde_yaml::{Mapping as YMap, Value as YamlValue},
    std::{
        iter,
        iter::{FromIterator, Iterator},
        mem,
        path::PathBuf,
    },
};

// Convenience macro for logging match arms
#[macro_export]
macro_rules! match_with_log {
    ( $val:expr, $log:expr) => {{
        $log;
        $val
    }};
}

// Transparent helper enum for hinting to the outwriter
// What the underlying data structure is
#[derive(Serialize)]
#[serde(untagged)]
pub enum Output {
    Json(JsonValue),
    Yaml(YamlValue),
}

// Helper function for building Json compliant memory representations
pub fn build_json(hdr: Vec<&str>, record_list: Vec<Record>) -> JsonValue {
    record_list
        .into_iter()
        .map(|record| {
            let mut headers = hdr.iter().take(record.field_count as usize);
            let mut records = record.data.iter();
            let mut output = JMap::new();
            loop {
                let h_item = headers.next();
                let r_item = records.next();
                trace!("header: {:?}, field: {:?}", h_item, r_item);

                if h_item != None || r_item != None {
                    let h_json = match h_item {
                        Some(hdr) => hdr,
                        None => "",
                    };
                    let r_json = match r_item {
                        Some(rcd) => rcd,
                        None => "",
                    };
                    output.insert(h_json.to_string(), JsonValue::String(r_json.to_string()));
                } else {
                    break;
                }
            }
            trace!("Map contents: {:?}", &output);

            output
        })
        .collect::<JsonValue>()
}

// Helper function for building Yaml compliant memory representations
pub fn build_yaml(hdr: Vec<&str>, record_list: Vec<Record>) -> YamlValue {
    record_list
        .into_iter()
        .map(|record| {
            let mut headers = hdr.iter().take(record.field_count as usize);
            let mut records = record.data.iter();
            let mut output = YMap::new();
            loop {
                let h_item = headers.next();
                let r_item = records.next();
                trace!("header: {:?}, field: {:?}", h_item, r_item);

                if h_item != None || r_item != None {
                    let h_json = match h_item {
                        Some(hdr) => hdr,
                        None => "",
                    };
                    let r_json = match r_item {
                        Some(rcd) => rcd,
                        None => "",
                    };
                    output.insert(
                        YamlValue::String(h_json.to_string()),
                        YamlValue::String(r_json.to_string()),
                    );
                } else {
                    break;
                }
            }
            trace!("Map contents: {:?}", &output);

            output
        })
        .collect::<YamlValue>()
}

// Supported read source options
#[derive(Debug)]
pub enum ReadFrom {
    File(PathBuf),
    Stdin,
}

// Displays either 'Stdin' or a file, if file contains non ASCII
// characters, they are replaced with � (U+FFFD)
impl std::fmt::Display for ReadFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let display = match self {
            ReadFrom::File(path) => format!(
                "File: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            ReadFrom::Stdin => format!("Stdin"),
        };

        write!(f, "{}", display)
    }
}

// In-program representation of a record and relevant metadata
pub struct Record {
    pub data: Vec<String>,
    pub field_count: u64,
}

impl FromIterator<(u64, String)> for Record {
    fn from_iter<I: IntoIterator<Item = (u64, String)>>(iter: I) -> Self {
        // Shadowed iter here
        let iter = iter.into_iter();
        let mut field_count = 0u64;
        let mut data = match iter.size_hint() {
            (_, Some(ub)) => Vec::with_capacity(ub),
            (lb, None) => Vec::with_capacity(lb),
        };

        for (c, v) in iter {
            data.push(v);

            if c > field_count {
                field_count = c
            }
        }

        Record { data, field_count }
    }
}

// Supported serialization formats
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Json,
    JsonPretty,
    Yaml,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let display = match *self {
            OutputFormat::Json => "Json",
            OutputFormat::JsonPretty => "Pretty Json",
            OutputFormat::Yaml => "Yaml",
        };

        write!(f, "{}", display)
    }
}

// Custom iterator interface for checking if an item
// is the first or last item in an iterator
// returns a tuple -> (is_first, is_last, item): (bool, bool, I: Iterator)
pub trait IdentifyFirstLast: Iterator + Sized {
    fn identify_first_last(self) -> FirstLast<Self>;
}

impl<I> IdentifyFirstLast for I
where
    I: Iterator,
{
    fn identify_first_last(self) -> FirstLast<Self> {
        FirstLast(true, self.peekable())
    }
}

pub struct FirstLast<I>(bool, iter::Peekable<I>)
where
    I: Iterator;

impl<I> Iterator for FirstLast<I>
where
    I: Iterator,
{
    type Item = (bool, bool, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let first = mem::replace(&mut self.0, false);
        self.1.next().map(|e| (first, self.1.peek().is_none(), e))
    }
}
