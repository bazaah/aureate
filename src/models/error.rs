use std::{error::Error, fmt::Debug, io::Error as ioError, ops::Try, process::Termination};

#[derive(Debug)]
pub(crate) enum ErrorKind {
    Generic,
    ThreadFailed(String),
    UnexpectedChannelClose(String),
    Io(ioError),
    ParseYaml(serde_yaml::Error),
}

impl From<ErrorKind> for i32 {
    fn from(err: ErrorKind) -> Self {
        match err {
            ErrorKind::Generic => 1,
            ErrorKind::Io(_) => 1,
            ErrorKind::ParseYaml(_) => 1,
            ErrorKind::ThreadFailed(_) => 2,
            ErrorKind::UnexpectedChannelClose(_) => 3,
        }
    }
}

impl From<ioError> for ErrorKind {
    fn from(err: ioError) -> Self {
        ErrorKind::Io(err)
    }
}

impl From<serde_yaml::Error> for ErrorKind {
    fn from(err: serde_yaml::Error) -> Self {
        ErrorKind::ParseYaml(err)
    }
}

impl From<serde_json::Error> for ErrorKind {
    fn from(err: serde_json::Error) -> Self {
        use serde_json::error::Category;
        match err.classify() {
            Category::Io | Category::Data | Category::Syntax | Category::Eof => {
                ErrorKind::Io(err.into())
            }
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
        match self {
            ErrorKind::Generic => write!(f, "Generic Error"),
            ErrorKind::ThreadFailed(e) => write!(f, "Thread: {} failed to return", e),
            ErrorKind::UnexpectedChannelClose(e) => write!(f, "A channel quit unexpectedly: {}", e),
            ErrorKind::Io(e) => write!(f, "An underlying IO error occurred: {}", e),
            ErrorKind::ParseYaml(e) => write!(f, "An underlying IO (yml) error occurred: {}", e),
        }
    }
}

impl Error for ErrorKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Figure this out later
        match self {
            ErrorKind::Generic => None,
            ErrorKind::ThreadFailed(_) => None,
            ErrorKind::UnexpectedChannelClose(_) => None,
            ErrorKind::Io(e) => Some(e),
            ErrorKind::ParseYaml(e) => Some(e),
        }
    }
}

pub(crate) enum ProgramExit<T>
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
