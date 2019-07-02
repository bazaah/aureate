use {
    csv::StringRecord,
    serde::Serialize,
    serde_json::value::Value as JsonValue,
    serde_yaml::Value as YamlValue,
    std::{
        collections::BTreeSet,
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

#[derive(Clone)]
pub struct Headers {
    list: Vec<String>,
    length: usize,
}

impl Headers {
    pub fn new(unparsed_list: &StringRecord) -> Self {
        let list: Vec<String> = unparsed_list.iter().map(|csv| csv.to_string()).collect();
        let length = list.len();
        Headers { list, length }
    }

    pub fn length(&self) -> u64 {
        self.length as u64
    }

    pub fn list_copy(&self) -> Vec<String> {
        self.list.clone()
    }

    pub fn extend(&mut self, max_fields: u64) {
        let mut iter_binding_a;
        let mut iter_binding_b;
        let iter: &mut dyn Iterator<Item = (usize, String)> = match max_fields > self.length() {
            true => {
                let additional = (self.length() + 1..=max_fields)
                    .into_iter()
                    .map(|num| format!("__HEADER__{}", num));
                iter_binding_a = self.list.iter().cloned().chain(additional).enumerate();
                &mut iter_binding_a
            }
            false => {
                iter_binding_b = self.list.iter().cloned().enumerate();
                &mut iter_binding_b
            }
        };

        let extended = iter
            .scan(BTreeSet::new(), |dictionary, (index, header)| {
                if !dictionary.insert(header.clone()) {
                    let replacement = format!("__HEADER__{}", index);
                    let tail = match index {
                        i if i == 1 => format_args!("st"),
                        i if i == 2 => format_args!("nd"),
                        i if i == 3 => format_args!("rd"),
                        _ => format_args!("th"),
                    };
                    warn!(
                        "{}{} header is a duplicate! replacing [{}] with: [{}]",
                        index, tail, &header, replacement
                    );

                    dictionary.insert(replacement.clone());
                    Some(replacement)
                } else {
                    Some(header)
                }
            })
            .collect::<Vec<String>>();

        self.transmute(extended);
    }

    fn transmute(&mut self, replacement: Vec<String>) {
        let new_list = replacement;
        let new_length = new_list.len();

        std::mem::replace(&mut self.list, new_list);
        self.length = new_length;
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
