use std::convert;
use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::result;

/// A convenient typedef of the return value of any `Benc`ode action
pub type Result<T> = result::Result<T, Error>;

/// Indicates various errors
#[derive(Debug)]
pub enum Error {
    /// Generic IoError
    Io(io::Error),
    /// Generic error
    Other(&'static str),

    #[doc(hidden)]
    /// For internal use only
    Delim(u8),
    /// For internal use only
    EndOfFile,
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (&Error::Delim(s), &Error::Delim(o)) => s == o,
            (&Error::Other(s), &Error::Other(o)) => s == o,
            (&Error::Io(ref s), &Error::Io(ref o)) => s.kind() == o.kind(),
            (&Error::EndOfFile, &Error::EndOfFile) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Io(ref e) => e.description(),
            Error::Other(e) => e,
            Error::Delim(_) => "Delimiter reached",
            Error::EndOfFile => "End of file",
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::Io(ref e) => Some(e),
            _ => None,
        }
    }
}


impl convert::From<u8> for Error {
    fn from(err: u8) -> Error {
        Error::Delim(err)
    }
}

impl convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl convert::From<&'static str> for Error {
    fn from(err: &'static str) -> Error {
        Error::Other(err)
    }
}
