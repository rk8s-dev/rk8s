use std::fmt;

pub type AardvarkResult<T> = Result<T, AardvarkError>;

#[derive(Debug)]
pub enum AardvarkError {
    Message(String),
    IOError(std::io::Error),
    Chain(String, Box<Self>),
    List(AardvarkErrorList),
    AddrParseError(std::net::AddrParseError),
}

impl AardvarkError {
    pub fn msg<S>(msg: S) -> Self
    where
        S: Into<String>,
    {
        Self::Message(msg.into())
    }

    pub fn wrap<S>(msg: S, chained: Self) -> Self
    where
        S: Into<String>,
    {
        Self::Chain(msg.into(), Box::new(chained))
    }
}

pub trait AardvarkWrap<T, E> {
    /// Wrap the error value with additional context.
    fn wrap<C>(self, context: C) -> AardvarkResult<T>
    where
        C: Into<String>,
        E: Into<AardvarkError>;
}

impl<T, E> AardvarkWrap<T, E> for Result<T, E>
where
    E: Into<AardvarkError>,
{
    fn wrap<C>(self, msg: C) -> AardvarkResult<T>
    where
        C: Into<String>,
        E: Into<AardvarkError>,
    {
        // Not using map_err to save 2 useless frames off the captured backtrace
        // in ext_context.
        match self {
            Ok(ok) => Ok(ok),
            Err(error) => Err(AardvarkError::wrap(msg, error.into())),
        }
    }
}

impl fmt::Display for AardvarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(s) => write!(f, "{s}"),
            Self::Chain(s, e) => write!(f, "{s}: {e}"),
            Self::IOError(e) => write!(f, "IO error: {e}"),
            Self::AddrParseError(e) => write!(f, "parse address: {e}"),
            Self::List(list) => {
                // some extra code to only add \n when it contains multiple errors
                let mut iter = list.0.iter();
                if let Some(first) = iter.next() {
                    write!(f, "{first}")?;
                }
                for err in iter {
                    write!(f, "\n{err}")?;
                }
                Ok(())
            }
        }
    }
}

impl From<std::io::Error> for AardvarkError {
    fn from(err: std::io::Error) -> Self {
        Self::IOError(err)
    }
}

impl From<nix::Error> for AardvarkError {
    fn from(err: nix::Error) -> Self {
        Self::IOError(err.into())
    }
}

impl From<std::net::AddrParseError> for AardvarkError {
    fn from(err: std::net::AddrParseError) -> Self {
        Self::AddrParseError(err)
    }
}

#[derive(Debug)]
pub struct AardvarkErrorList(Vec<AardvarkError>);

impl AardvarkErrorList {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn push(&mut self, err: AardvarkError) {
        self.0.push(err)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// we do not need it but clippy wants it
impl Default for AardvarkErrorList {
    fn default() -> Self {
        Self::new()
    }
}
