/// Python exceptions that trafilatura's control flow depends on (it catches some kinds and not others).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// ValueError: invalid XML characters on assignment, encoding declarations in str input, cycles.
    Value(String),
    /// TypeError.
    Type(String),
    /// lxml.etree.ParserError: "Document is empty", fragment_fromstring shape errors.
    Parser(String),
    /// lxml.etree.XMLSyntaxError: libxml2 returned no document.
    Syntax(String),
    /// AttributeError / IndexError / AssertionError raised by Python code paths being mirrored.
    Runtime(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}
