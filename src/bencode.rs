//! Decode and encode bencoded values as described by [BEP 003](
//! http://www.bittorrent.org/beps/bep_0003.html).

use std::io;
use std::error;
// TODO - Consider changing Map types
use std::collections::TreeMap;
use std::io::extensions::Bytes;

/// Similar to the try! macro from stdlib but converts the `Ok` value to a Benc
macro_rules! into_benc(
    ($node:expr) => (
        match $node {
            Ok(n)  => n.into_benc(),
            Err(e) => return Err(::std::error::FromError::from_error(e)),
        }
    );
)

/// A convenient typedef of the return value of any `Benc`ode action
pub type BencResult<T> = Result<T, BencError>;

/// Indicates various errors
#[deriving(Show, PartialEq, Send)]
pub enum BencError {
    /// Delimiter
    Delim(u8),
    /// Special value if `IoError.kind` is EndOfFile
    EOF(io::IoError),
    /// Generic IoError
    IoError(io::IoError),
    /// Generic error
    OtherErr(&'static str),
}

impl error::Error for BencError {
    fn description(&self) -> &str {
        match *self {
            BencError::Delim(_)       => "Reached delimiter",
            BencError::EOF(ref e)     => e.description(),
            BencError::IoError(ref e) => e.description(),
            BencError::OtherErr(e)    => e,
        }
    }

    fn detail(&self) -> Option<String> {
        match *self {
            BencError::Delim(e)       => Some(format!("Reached delimiter {}", e as char)),
            BencError::EOF(ref e)     => e.detail(),
            BencError::IoError(ref e) => e.detail(),
            BencError::OtherErr(e)    => Some(e.to_string()),
        }
    }
}

impl error::FromError<u8> for BencError {
    fn from_error(err: u8) -> BencError {
        BencError::Delim(err)
    }
}

impl error::FromError<io::IoError> for BencError {
    fn from_error(err: io::IoError) -> BencError {
        match err.kind {
            io::IoErrorKind::EndOfFile => BencError::EOF(err),
            _                          => BencError::IoError(err),
        }
    }
}

impl error::FromError<&'static str> for BencError {
    fn from_error(err: &'static str) -> BencError {
        BencError::OtherErr(err)
    }
}

impl<T> error::FromError<u8> for BencResult<T> {
    fn from_error(err: u8) -> BencResult<T> {
        Err(error::FromError::from_error(err))
    }
}

impl<T> error::FromError<io::IoError> for BencResult<T> {
    fn from_error(err: io::IoError) -> BencResult<T> {
        Err(error::FromError::from_error(err))
    }
}

impl<T> error::FromError<&'static str> for BencResult<T> {
    fn from_error(err: &'static str) -> BencResult<T> {
        Err(error::FromError::from_error(err))
    }
}

/// Indicates type of the Benc node
#[deriving(PartialEq)]
enum NodeType {
    TString,
    TInt,
    TList,
    TDict,
}

impl NodeType {
    /// Returns the bencoded type of `c`
    fn type_of(c: u8) -> Option<NodeType> {
        match c {
            b'0'...b'9' => Some(NodeType::TString),
            b'i'        => Some(NodeType::TInt),
            b'l'        => Some(NodeType::TList),
            b'd'        => Some(NodeType::TDict),
            _           => None,
        }
    }
}

/// The types that can be represented as a bencoded values
#[deriving(Show, PartialEq)]
pub enum Benc {
    BString(Vec<u8>),
    BInt(i64),
    BList(Vec<Benc>),
    BDict(TreeMap<Vec<u8>, Benc>),
}

impl Benc {
    /// Consumes the Reader and builds a Vec of `Benc` values. The function will return early if
    /// an invalid Benc node is found.
    pub fn build_tree<R>(bytes: &mut Bytes<R>) -> BencResult<Vec<Benc>> where R: Reader {
        let mut ast = Vec::new();

        loop {
            // TODO - Handle '\n'?
            let node = match Benc::node(bytes, Some(b'\n')) {
                Ok(n)                    => n,
                Err(BencError::EOF(_))   => return Ok(ast),
                Err(BencError::Delim(_)) => continue,
                Err(e)                   => return Err(e),
            };
            ast.push(node);
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded string. `c` is an optional
    /// first byte of the string.
    fn benc_string<R>(bytes: &mut Bytes<R>, c: Option<u8>) -> BencResult<Vec<u8>> where R: Reader {
        // Valid - 5:hello
        let mut buf = String::with_capacity(4);
        let err     = Err(BencError::OtherErr("Invalid string bencoding"));

        match c {
            None                => (),
            Some(c@b'0'...b'9') => unsafe { buf.as_mut_vec().push(c) },
            _                   => return err,
        }

        // Collect all numbers until ':'
        for c in *bytes {
            match c {
                Ok(c@b'0'...b'9') => unsafe { buf.as_mut_vec().push(c) },
                Ok(b':')          => break,
                Ok(_)             => return err,
                Err(e)            => return error::FromError::from_error(e),
            }
        }

        // returns `None` if buf is empty
        let mut len = match from_str(buf[]) {
            Some(l) => l,
            None    => return err,
        };
        let mut buf = Vec::with_capacity(len);

        // Read `len` bytes
        for c in *bytes {
            match c {
                Ok(c)  => buf.push(c),
                Err(e) => return error::FromError::from_error(e),
            }

            len -= 1;
            if len == 0 {
                break;
            }
        }

        match len {
            0 => Ok(buf),
            _ => err,
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded int
    fn benc_int<R>(bytes: &mut Bytes<R>) -> BencResult<i64> where R: Reader {
        // Valid   - i5e, i0e
        // Invalid - i05e, i00e ie
        let mut buf = String::with_capacity(4);
        let err     = Err(BencError::OtherErr("Invalid int bencoding"));

        // Only the first char can be '-'
        // Known to be valid ASCII
        unsafe {
            buf.as_mut_vec().push(match bytes.next() {
                Some(Ok(c@b'0'...b'9')) => c,
                Some(Ok(c@b'-'))        => c,
                Some(Ok(_))             => return err,
                Some(Err(e))            => return error::FromError::from_error(e),
                None                    => return err,
            });
        }

        // Read numbers until 'e'
        for c in *bytes {
            match c {
                Ok(c@b'0'...b'9') => unsafe { buf.as_mut_vec().push(c) },
                Ok(b'e')          => break,
                Ok(_)             => return err,
                Err(e)            => return error::FromError::from_error(e),
            }
        }

        if buf.len() == 0  // 'ie' is invalid
            || (buf.as_bytes()[0] == b'0' && buf.len() > 1) // i05e is invalid
            || (buf.len() > 1 && buf[..2] == "-0")          // i-0e is invalid
            { return err; }

        match from_str(buf[]) {
            Some(n) => Ok(n),
            None    => err,
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded list
    fn benc_list<R>(bytes: &mut Bytes<R>) -> BencResult<Vec<Benc>> where R: Reader {
        // Valid - l[Node]+e
        let mut list = Vec::new();

        loop {
            let node = match Benc::node(bytes, Some(b'e')) {
                Ok(n)                    => n,
                Err(BencError::Delim(_)) => return Ok(list),
                Err(e)                   => return Err(e),
            };
            list.push(node);
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded dictionary. Dictionary keys
    /// should be `Benc::BString`s
    fn benc_dict<R>(bytes: &mut Bytes<R>) -> BencResult<TreeMap<Vec<u8>, Benc>> where R: Reader {
        // Valid - d(<NString><Node>)+e
        let mut dict     = TreeMap::new();
        let mut prev_key = Vec::new(); // ensure keys are in alphabetical order
        let err          = Err(BencError::OtherErr("Invalid dict bencoding"));

        loop {
            // Key
            prev_key = match Benc::node(bytes, Some(b'e')) {
                Ok(Benc::BString(n))     => if n > prev_key { n } else { return err; },
                Ok(_)                    => return Err(BencError::OtherErr(
                                                "Expected `BString` key for dictionary")),
                Err(BencError::Delim(_)) => return Ok(dict),
                Err(e)                   => return Err(e),
            };
            let k = prev_key.clone();

            // Value
            let val = match Benc::node(bytes, None) {
                Ok(n)  => n,
                Err(e) => return Err(e),
            };

            // Returns `Some(val)` if `val` existed already
            if dict.insert(k, val).is_some() {
                return err;
            }
        }
    }

    /// Consumes as much of `bytes` as needed to build a single `Benc`oded value. If `bytes` has
    /// nothing to read `BencError::EOF` is returned
    fn node<R>(bytes: &mut Bytes<R>, delim: Option<u8>) -> BencResult<Benc> where R: Reader {
        let err = Err(BencError::OtherErr("Parse error"));
        let eof = io::IoError {
            kind: io::IoErrorKind::EndOfFile,
            desc: "end of file",
            detail: None,
        };

        let c = match bytes.next() {
            Some(Ok(c)) if Some(c) == delim => return error::FromError::from_error(c),
            Some(Ok(c))                     => c,
            Some(Err(e))                    => return error::FromError::from_error(e),
            None                            => return error::FromError::from_error(eof),
        };

        match NodeType::type_of(c) {
            Some(NodeType::TString) => Ok(into_benc!(Benc::benc_string(bytes, Some(c)))),
            Some(NodeType::TInt)    => Ok(into_benc!(Benc::benc_int (bytes))),
            Some(NodeType::TList)   => Ok(into_benc!(Benc::benc_list(bytes))),
            Some(NodeType::TDict)   => Ok(into_benc!(Benc::benc_dict(bytes))),
            None                    => err,
        }
    }
}

// TODO - decode and encode traits/implementations

/// Trait to consume the value returning a `Benc` type
trait IntoBenc {
    fn into_benc(self) -> Benc;
}

impl IntoBenc for String {
    fn into_benc(self) -> Benc { Benc::BString(self.into_bytes()) }
}

impl IntoBenc for Vec<u8> {
    fn into_benc(self) -> Benc{ Benc::BString(self) }
}

impl IntoBenc for i64 {
    fn into_benc(self) -> Benc { Benc::BInt(self) }
}

impl IntoBenc for Vec<Benc> {
    fn into_benc(self) -> Benc { Benc::BList(self) }
}

impl IntoBenc for TreeMap<Vec<u8>, Benc> {
    fn into_benc(self) -> Benc { Benc::BDict(self) }
}

#[cfg(test)]
mod tests {
    use std::fmt::Show;
    use std::io::extensions::Bytes;
    use std::io::MemReader;

    use super::{Benc, BencResult, BencError};
    use super::Benc as B;

    macro_rules! hashmap(
        ($($k:expr => $v:expr),*) => ({
            let mut d = ::std::collections::TreeMap::new();
            $(d.insert($k, $v);)*
            d
        });

        ($($k:expr => $v:expr),+,) => (hashmap!($($k => $v),+));
    )

    macro_rules! bytes(
        ($s:expr) => (
            $s.into_string().into_bytes()
        );
    )

    #[test]
    fn benc_string() {
        let is_valid = |data: &str, first: Option<u8>| {
            let bind = |brd: &mut Bytes<MemReader>| -> BencResult<Vec<u8>> {
                Benc::benc_string(brd, first)
            };
            let expect = data.splitn(1, ':')
                .skip(1)
                .collect::<Vec<&str>>()
                .concat()
                .into_string();

            assert(bind, data, Ok(bytes!(expect)));
        };

        let is_invalid = |data: &str, first: Option<u8>| {
            let bind = |brd: &mut Bytes<MemReader>| -> BencResult<Vec<u8>> {
                Benc::benc_string(brd, first)
            };

            assert(bind, data, Err(BencError::OtherErr("Mock data")));
        };

        is_valid("7:yahallo", None);
        is_valid("15:こんにちわ", None); // Bytes, not chars
        is_valid(":\"hello\"", Some(b'7'));
        is_valid("1:hellohello1", Some(b'1'));
        is_valid("2:hi", Some(b'0'));

        is_invalid("6:hello", None);
        is_invalid("5:hallo", Some(b'a'));
        is_invalid("", None);
    }

    #[test]
    fn benc_int() {
        let is_valid = |expect: i64| {
            assert(Benc::benc_int, format!("{}e", expect)[], Ok(expect));
        };

        // Valid
        is_valid(2<<48);
        is_valid(-2<<48);
        is_valid(0);

        // Invalid
        assert(Benc::benc_int, "e",   Err(BencError::OtherErr("Mock data")));
        assert(Benc::benc_int, "-0e", Err(BencError::OtherErr("Mock data")));
        assert(Benc::benc_int, "00e", Err(BencError::OtherErr("Mock data")));
        assert(Benc::benc_int, "05e", Err(BencError::OtherErr("Mock data")));
    }

    #[test]
    fn benc_list() {
        assert(Benc::benc_list,
            "5:helloi42ee",
            Ok(vec!(
                B::BString(bytes!("hello")),
                B::BInt(42),
            ))
        );

        assert(Benc::benc_list,
            "5:helloi42eli2ei3e2:hid4:listli1ei2ei3ee7:yahallo2::)eed2:hi5:hello3:inti15eee",
            Ok(vec!(
                B::BString(bytes!("hello")),
                B::BInt(42),
                B::BList(vec!(
                    B::BInt(2),
                    B::BInt(3),
                    B::BString(bytes!("hi")),
                    B::BDict(hashmap!(
                        bytes!("list")    => B::BList(vec!(B::BInt(1), B::BInt(2), B::BInt(3))),
                        bytes!("yahallo") => B::BString(bytes!(":)")),
                    )),
                )),
                B::BDict(hashmap!(
                    bytes!("hi")  => B::BString(bytes!("hello")),
                    bytes!("int") => B::BInt(15),
                )),
            ))
        );

        assert(Benc::benc_list, "5:helloi4e", Err(BencError::OtherErr("Mock data")));
    }

    #[test]
    fn benc_dict() {
        assert(Benc::benc_dict,
            "2:hi5:helloe",
            Ok(hashmap!(
                bytes!("hi") => B::BString(bytes!("hello")),
            ))
        );

        assert(Benc::benc_dict,
            concat!("10:dictionaryd2:hi5:hello3:inti15ee7:integeri42e4:listli2ei3e2:hid4:listli",
                    "1ei2ei3ee7:yahallo2::)ee3:str5:helloe"),
            Ok(hashmap!(
                bytes!("str")     => B::BString(bytes!("hello")),
                bytes!("integer") => B::BInt(42),
                bytes!("list")    => B::BList(vec!(
                    B::BInt(2),
                    B::BInt(3),
                    B::BString(bytes!("hi")),
                    B::BDict(hashmap!(
                        bytes!("list")    => B::BList(vec!(B::BInt(1), B::BInt(2), B::BInt(3))),
                        bytes!("yahallo") => B::BString(bytes!(":)")),
                    )),
                )),
                bytes!("dictionary") => B::BDict(hashmap!(
                    bytes!("hi")  => B::BString(bytes!("hello")),
                    bytes!("int") => B::BInt(15i64),
                )),
            ))
        );

        assert(Benc::benc_dict, "2:hi5:hello1:ai32ee", Err(BencError::OtherErr(("Mock data"))));
    }

    fn assert<T, E>(func: |&mut Bytes<MemReader>| -> Result<T, E>,
                    data: &str,
                    expect: Result<T, E>)
        where T: PartialEq+Show, E: PartialEq+Show
    {
        let mut brd = MemReader::new(bytes!(data));
        let result  = func(&mut Bytes::new(&mut brd));

        match result {
            Ok(_)  => assert!(result == expect, "{} == {}", result, expect),
            Err(_) => assert!(expect.is_err(),  "{} == {}", result, expect),
        }
    }
}

