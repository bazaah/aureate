use std::{error::Error, fmt::Debug, ops::Try, process::Termination};

#[derive(Debug, Clone)]
pub(crate) enum ErrorKind {
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
