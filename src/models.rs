use {
    crate::cli::ProgramArgs,
    csv::ReaderBuilder,
    serde::Serialize,
    serde_json::{map::Map, value::Value as JVal},
    std::{
        boxed::Box,
        collections::BTreeMap,
        error::Error,
        fmt::Debug,
        fs::File,
        io::{stdin as cin, stdout as cout, Read as ioRead, Write as ioWrite},
        iter,
        iter::FromIterator,
        mem,
        ops::Try,
        path::PathBuf,
        process::Termination,
        vec::Vec,
    },
};

// Convenience macro for logging match arms
macro_rules! match_with_log {
    ( $val:expr, $log:expr) => {{
        $log;
        $val
    }};
}

// Determines write destination from runtime args
pub fn get_writer(w: &Option<String>) -> Box<dyn ioWrite> {
    match w {
        Some(file_name) => match_with_log!(
            match File::create(file_name).ok() {
                Some(file) => match_with_log!(Box::new(file), info!("Success!")),
                None => match_with_log!(Box::new(cout()), warn!("Failed! Switching to stdout...")),
            },
            info!("Attempting to create {}...", file_name)
        ),
        None => match_with_log!(
            Box::new(cout()),
            info!("No file detected, defaulting to stdout...")
        ),
    }
}

// Supported read source options
#[derive(Debug)]
pub enum ReadFrom {
    File(PathBuf),
    Stdin,
}

// Helper function for generating a list of read sources at runtime
pub fn get_reader(r: Option<&str>) -> Option<ReadFrom> {
    match r {
        Some("-") => Some(ReadFrom::Stdin),
        Some(file_name) => {
            let path = PathBuf::from(file_name);
            if path.is_file() {
                Some(ReadFrom::File(path))
            } else {
                None
            }
        }
        None => Some(ReadFrom::Stdin),
    }
}

// Opens a read source, defaults to stdin if source errors
pub fn set_reader(src: &Option<ReadFrom>) -> Box<dyn ioRead> {
    match src {
        Some(s) => match s {
            ReadFrom::File(path) => match_with_log!(
                match File::open(path) {
                    Ok(f) => match_with_log!(Box::new(f), info!("Success!")),
                    Err(e) => match_with_log!(
                        Box::new(cin()),
                        warn!("Failed! {}, switching to stdin...", e)
                    ),
                },
                info!("Attempting to read from {:?}...", path)
            ),
            ReadFrom::Stdin => match_with_log!(Box::new(cin()), info!("Reading CSV from stdin...")),
        },
        None => match_with_log!(
            Box::new(cin()),
            info!("No input source found, defaulting to stdin...")
        ),
    }
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

// Parses CSV source into a manipulatable format
// that other functions can use to build JSON/YAML structures
pub fn csv_from_source<R>(
    opts: &ProgramArgs,
    src: R,
) -> Result<(Vec<String>, Vec<Vec<String>>), Box<dyn Error>>
where
    R: ioRead,
{
    let mut rdr = ReaderBuilder::new()
        .delimiter(opts.delimiter())
        .flexible(opts.flexible())
        .escape(opts.escape())
        .comment(opts.comment())
        .quote(opts.quote())
        .trim(opts.trim_settings())
        .double_quote(opts.quote_settings().0)
        .quoting(opts.quote_settings().1)
        .from_reader(src);

    // Keeping track of highest row length
    let mut highest_num_fields = 0u64;

    // Parse records
    let records = rdr
        .records()
        // Skip rows which error based on the CSV parser options, with a warning
        .filter_map(|result| match result {
            Ok(r) => Some(r),
            Err(e) => match_with_log!(None, warn!("Failed to parse record: {}, skipping...", e)),
        })
        // Parse CSV into a useable format and add metadata necessary for the conversion
        .map(|record| {
            record
                .iter()
                .identify_first_last()
                .map(|(_, last, field)| (last, field.to_string()))
                .scan(0u64, |count, (last_record, record)| {
                    *count += 1;
                    if last_record == true {
                        Some((*count, record))
                    } else {
                        Some((0, record))
                    }
                })
                .collect::<RecordWrapper>()
        })
        .inspect(|wrapper| {
            if highest_num_fields < wrapper.highest {
                highest_num_fields = wrapper.highest;
            }
        })
        // Strip CSV data from internal structures
        .map(|wrapper| {
            let RecordWrapper { record, .. } = wrapper;
            record
        })
        .collect::<Vec<Vec<String>>>();

    info!("Highest record field length: {}", highest_num_fields);

    //Parse headers
    let h = rdr.headers()?;
    let headers: Vec<String>;
    let h_count = h.len();
    let hnf = highest_num_fields as usize;

    // If row length is non-uniform add additional placeholder rows
    if hnf > h_count {
        let additional = (h_count + 1..=hnf)
            .into_iter()
            .map(|num| format!("FIELD_{}", num));
        let tmp = h
            .iter()
            .map(|h| h.to_string())
            .chain(additional)
            .collect::<Vec<String>>();
        headers = tmp
    // Otherwise use the standard headers
    } else {
        let tmp = h.iter().map(|h| h.to_string()).collect::<Vec<String>>();

        headers = tmp
    }

    Ok((headers, records))
}

struct RecordWrapper {
    pub highest: u64,
    pub record: Vec<String>,
}

impl FromIterator<(u64, String)> for RecordWrapper {
    fn from_iter<I: IntoIterator<Item = (u64, String)>>(iter: I) -> Self {
        let mut highest = 0u64;
        let mut record = Vec::new();

        for (c, v) in iter {
            record.push(v);

            if c > highest {
                highest = c
            }
        }

        RecordWrapper { highest, record }
    }
}

// JSON builder function, as JSON is a subset (mostly)
// of YAML this function also builds YAML representable data
pub fn compose(
    _opts: &ProgramArgs,
    data: (Vec<String>, Vec<Vec<String>>),
) -> Vec<Map<String, JVal>> {
    let (header, record_list) = data;
    let hdr = header.iter().map(|s| &**s).collect::<Vec<&str>>();

    record_list
        .into_iter()
        .map(|record| {
            let mut headers = hdr.iter();
            let mut records = record.iter();
            let mut output = Map::new();
            loop {
                let h_item = headers.next();
                let r_item = records.next();
                trace!("header: {:?}, record: {:?}", h_item, r_item);

                if h_item != None || r_item != None {
                    let h_json = match h_item {
                        Some(hdr) => hdr,
                        None => "",
                    };
                    let r_json = match r_item {
                        Some(rcd) => rcd,
                        None => "",
                    };
                    output.insert(h_json.to_string(), JVal::String(r_json.to_string()));
                } else {
                    break;
                }
            }
            trace!("Map contents: {:?}", &output);

            output
        })
        .collect::<Vec<Map<String, JVal>>>()
}

// Serialization of the composed data occurs here
pub fn outwriter<W, S: ?Sized>(
    writer: W,
    output: &S,
    format: &OutputFormat,
) -> Result<(), Box<dyn Error>>
where
    W: ioWrite,
    S: Serialize,
{
    match *format {
        OutputFormat::JsonPretty => match_with_log!(
            match serde_json::to_writer_pretty(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using pretty Json writer")
        ),
        OutputFormat::Json => match_with_log!(
            match serde_json::to_writer(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using Json writer")
        ),
        OutputFormat::Yaml => match_with_log!(
            match serde_yaml::to_writer(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using Yaml writer")
        ),
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

// Error Handling below
#[derive(Debug, Clone)]
pub enum ErrorKind {
    Generic,
}

impl From<ErrorKind> for i32 {
    fn from(err: ErrorKind) -> Self {
        match err {
            ErrorKind::Generic => 1,
        }
    }
}

impl From<std::option::NoneError> for ErrorKind {
    fn from(_: std::option::NoneError) -> Self {
        ErrorKind::Generic
    }
}

impl From<Box<dyn Error>> for ErrorKind {
    fn from(_: Box<dyn Error>) -> Self {
        ErrorKind::Generic
    }
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Generic Error")
    }
}

impl Error for ErrorKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

pub enum ProgramExit<T>
where
    T: Error,
{
    Success,
    Failure(T),
}

impl<T: Into<i32> + Debug + Error> Termination for ProgramExit<T> {
    fn report(self) -> i32 {
        match self {
            ProgramExit::Success => 0,
            ProgramExit::Failure(err) => {
                error!("Program exited with error: {}", err);
                err.into()
            }
        }
    }
}

impl<T: Error> Try for ProgramExit<T> {
    type Ok = ();
    type Error = T;

    fn into_result(self) -> Result<Self::Ok, Self::Error> {
        match self {
            ProgramExit::Success => Ok(()),
            ProgramExit::Failure(err) => Err(err),
        }
    }

    fn from_error(err: Self::Error) -> Self {
        ProgramExit::Failure(err)
    }

    fn from_ok(_: Self::Ok) -> Self {
        ProgramExit::Success
    }
}
