//! Decode and encode bencoded values as described by [BEP 003](
//! http://www.bittorrent.org/beps/bep_0003.html).
use std::io;
use std::convert;
use std::collections::HashMap;

use error;

/// Indicates type of the Benc node
#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeType {
    String,
    Int,
    List,
    Dict,
}

impl NodeType {
    /// Returns the bencoded type of `c`
    fn type_of(c: u8) -> Option<NodeType> {
        match c {
            b'0'...b'9' => Some(NodeType::String),
            b'i' => Some(NodeType::Int),
            b'l' => Some(NodeType::List),
            b'd' => Some(NodeType::Dict),
            _ => None,
        }
    }
}

/// The types that can be represented as a bencoded values
#[derive(Debug, PartialEq, Eq)]
pub enum Benc {
    String(Vec<u8>),
    Int(i64),
    List(Vec<Benc>),
    Dict(HashMap<Vec<u8>, Benc>),
}

impl Benc {
    /// Consumes the Reader and builds a Vec of `Benc` values. The function will return early if
    /// an invalid Benc node is found.
    pub fn new<R>(bytes: &mut io::Bytes<R>) -> error::Result<Vec<Benc>>
    where
        R: io::Read,
    {
        let mut ast = Vec::new();

        loop {
            let node = match Benc::node(bytes, None) {
                Ok(n) => n,
                Err(error::Error::EndOfFile) => return Ok(ast),
                Err(error::Error::Delim(_)) => continue,
                Err(e) => return Err(e),
            };
            ast.push(node);
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded string. `c` is the first
    /// byte of the string.
    fn string<R>(bytes: &mut io::Bytes<R>, c: u8) -> error::Result<Vec<u8>>
    where
        R: io::Read,
    {
        let err = Err(error::Error::Other("Invalid string bencoding"));
        let mut len = match c {
            c @ b'0'...b'9' => (c - b'0') as usize,
            _ => return err,
        };

        // read numbers until ':' and return early if any other character is read
        for c in bytes.by_ref() {
            match c {
                Ok(c @ b'0'...b'9') => match len.checked_mul(10)
                    .and_then(|n| n.checked_add((c - b'0') as usize))
                {
                    Some(n) => len = n,
                    None => return Err(error::Error::Other("Integer overflow")),
                },
                Ok(b':') => break,
                Ok(_) => return err,
                Err(e) => return Err(error::Error::from(e)),
            }
        }

        if len == 0 {
            return err;
        }

        let mut buf = Vec::with_capacity(len);

        // read `len` bytes, returning any error
        for c in bytes {
            match c {
                Ok(c) => buf.push(c),
                Err(e) => return Err(error::Error::from(e)),
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
    fn int<R>(bytes: &mut io::Bytes<R>) -> error::Result<i64>
    where
        R: io::Read,
    {
        let err = Err(error::Error::Other("Invalid int bencoding"));
        let mut num = 0;

        let neg = match bytes.next() {
            Some(Ok(b'-')) => -1,
            Some(Ok(c @ b'0'...b'9')) => {
                num = i64::from(c - b'0'); //  (c - b'0') as i64;
                1
            }
            Some(Ok(_)) | None => return err,
            Some(Err(e)) => return Err(error::Error::from(e)),
        };

        if neg == -1 {
            // 1..9 must follow -
            match bytes.next() {
                Some(Ok(c @ b'1'...b'9')) => {
                    num = i64::from(c - b'0');
                }
                Some(Err(e)) => return Err(error::Error::from(e)),
                _ => return err,
            }
        } else if num == 0 {
            // No digits may follow 0
            match bytes.next() {
                Some(Ok(b'e')) => return Ok(num),
                Some(Err(e)) => return Err(error::Error::from(e)),
                _ => return err,
            }
        }

        for c in bytes {
            match c {
                Ok(c @ b'0'...b'9') => match num.checked_mul(10)
                    .and_then(|n| n.checked_add(i64::from(c - b'0')))
                {
                    Some(n) => num = n,
                    None => return Err(error::Error::Other("Integer overflow")),
                },
                Ok(b'e') => return Ok(neg * num),
                Ok(_) => return err,
                Err(e) => return Err(error::Error::from(e)),
            }
        }

        err
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded list
    fn list<R>(bytes: &mut io::Bytes<R>) -> error::Result<Vec<Benc>>
    where
        R: io::Read,
    {
        let mut list = Vec::new();

        loop {
            match Benc::node(bytes, Some(b'e')) {
                Ok(n) => list.push(n),
                Err(error::Error::Delim(_)) => return Ok(list),
                Err(e) => return Err(e),
            }
        }
    }

    /// Consumes as much of `bytes` as needed to read a valid bencoded dictionary. Dictionary keys
    /// should be `Benc::BString`s
    fn dict<R>(bytes: &mut io::Bytes<R>) -> error::Result<HashMap<Vec<u8>, Benc>>
    where
        R: io::Read,
    {
        let mut dict = HashMap::new();
        let mut prev_key = Vec::new(); // ensure keys are in alphabetical order
        let err = Err(error::Error::Other("Invalid dict bencoding"));

        loop {
            let key = match Benc::node(bytes, Some(b'e')) {
                Ok(Benc::String(n)) => if n > prev_key {
                    n
                } else {
                    return err;
                },
                Ok(_) => return Err(error::Error::Other("Expected `BString` key for dictionary")),
                Err(error::Error::Delim(_)) => return Ok(dict),
                Err(e) => return Err(e),
            };

            // reuse prev_key's internal buffer if possible
            prev_key.clear();
            prev_key.extend(key.iter().cloned());

            // value
            let val = match Benc::node(bytes, None) {
                Ok(n) => n,
                Err(e) => return Err(e),
            };

            dict.insert(key, val);
        }
    }

    /// Consumes as much of `bytes` as needed to build a single `Benc`oded value. If `bytes` has
    /// nothing to read `Error::EOF` is returned
    fn node<R>(bytes: &mut io::Bytes<R>, delim: Option<u8>) -> error::Result<Benc>
    where
        R: io::Read,
    {
        let err = Err(error::Error::Other("Parse error"));

        let c = match bytes.next() {
            Some(Ok(c)) if Some(c) == delim => return Err(error::Error::Delim(c)),
            Some(Ok(0)) | None => return Err(error::Error::EndOfFile),
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(error::Error::Io(e)),
        };

        match NodeType::type_of(c) {
            Some(NodeType::String) => Ok(Benc::from(try!(Benc::string(bytes, c)))),
            Some(NodeType::Int) => Ok(Benc::from(try!(Benc::int(bytes)))),
            Some(NodeType::List) => Ok(Benc::from(try!(Benc::list(bytes)))),
            Some(NodeType::Dict) => Ok(Benc::from(try!(Benc::dict(bytes)))),
            None => err,
        }
    }
}

// Trait impl's to consume the value returning a `Benc` type
impl convert::From<String> for Benc {
    fn from(s: String) -> Benc {
        Benc::String(s.into_bytes())
    }
}

impl convert::From<Vec<u8>> for Benc {
    fn from(s: Vec<u8>) -> Benc {
        Benc::String(s)
    }
}

impl convert::From<i64> for Benc {
    fn from(s: i64) -> Benc {
        Benc::Int(s)
    }
}

impl convert::From<Vec<Benc>> for Benc {
    fn from(s: Vec<Benc>) -> Benc {
        Benc::List(s)
    }
}

impl convert::From<HashMap<Vec<u8>, Benc>> for Benc {
    fn from(s: HashMap<Vec<u8>, Benc>) -> Benc {
        Benc::Dict(s)
    }
}

#[cfg(test)]
mod test_nodetype {
    use super::NodeType;

    #[test]
    fn type_of() {
        for c in b'0'..b'9' + 1 {
            assert_eq!(Some(NodeType::String), NodeType::type_of(c))
        }

        for c in vec![
            (b'i', NodeType::Int),
            (b'l', NodeType::List),
            (b'd', NodeType::Dict),
        ] {
            assert_eq!(Some(c.1), NodeType::type_of(c.0))
        }

        // TODO - Use rand when stabilized to generate node values
        // let rng = rand::os::OsRng::new()
        //     .gen_iter()
        //     .filter(|r| match r {
        //         b'0'...b'9' => false,
        //         b'i' | b'l' | b'd' => false,
        //         _ => true,
        //     })
        //     .take(25);

        // for r in rng {
        //     assert_eq!(None, NodeType::type_of(r));
        // }
    }
}

#[cfg(test)]
mod test_benc {
    use std::borrow::ToOwned;
    use std::fmt::Debug;
    use std::io::{self, Read};

    use error;
    use super::Benc;
    use super::Benc as B;

    macro_rules! hashmap {
        ($($k:expr => $v:expr),*) => ({
            let mut d = ::std::collections::HashMap::new();
            $(d.insert($k, $v);)*
            d
        });

        ($($k:expr => $v:expr),+,) => (hashmap!($($k => $v),+));
    }

    macro_rules! bytes {
        ($s:expr) => ( $s.to_owned().into_bytes() );
    }

    #[test]
    fn new() {
        let data = concat!(
            "d8:announce40:http://tracker.example.com:8080/announce7:comment17:\"Hello mock data",
            "\"13:creation datei1234567890e9:httpseedsl31:http://direct.example.com/mock131:http",
            "://direct.example.com/mock2e4:infod6:lengthi562949953421312e4:name15:あいえおう12:p",
            "iece lengthi536870912eee").as_bytes();

        let expect = vec![
            B::Dict(hashmap!(
            bytes!("announce")      => B::String(bytes!("http://tracker.example.com:8080/announce")),
            bytes!("comment")       => B::String(bytes!("\"Hello mock data\"")),
            bytes!("creation date") => B::Int(1234567890),
            bytes!("httpseeds")     => B::List(vec!(
                B::String(bytes!("http://direct.example.com/mock1")),
                B::String(bytes!("http://direct.example.com/mock2")),
            )),
            bytes!("info") => B::Dict(hashmap!(
                bytes!("length")       => B::Int(562949953421312),
                bytes!("name")         => B::String(bytes!("あいえおう")),
                bytes!("piece length") => B::Int(536870912),
            )),
        )),
        ];

        let expect = Ok(expect);
        let result = Benc::new(&mut data.bytes());

        assert!(result == expect, "{:?} == {:?}", result, expect);
    }

    #[test]
    fn string() {
        fn is_valid(data: &str, first: u8) {
            let expect = data.splitn(2, |b| b == ':').nth(1).unwrap();

            assert(
                |brd| Benc::string(brd, first),
                data.as_bytes().bytes(),
                Ok(bytes!(expect)),
            );
        }

        fn is_invalid(data: &str, first: u8) {
            assert(
                |brd| Benc::string(brd, first),
                data.as_bytes().bytes(),
                Err(error::Error::Other("Mock data")),
            );
        }

        is_valid(":yahallo", b'7');
        is_valid("5:こんにちわ", b'1'); // bytes, not chars
        is_valid(":\"hello\"", b'7');
        is_valid("1:hellohello1", b'1');
        is_valid("2:hi", b'0');

        is_invalid(":hello", b'6');
        is_invalid("5:hallo", b'a');
        is_invalid("", b'a');
        is_invalid("8446744073709551616:overflow", b'1') // u64::MAX + 1
    }

    #[test]
    fn int() {
        fn is_valid(expect: i64) {
            assert(
                Benc::int,
                format!("{}e", expect).as_bytes().bytes(),
                Ok(expect),
            );
        }

        fn is_invalid(data: &str) {
            assert(
                Benc::int,
                data.as_bytes().bytes(),
                Err(error::Error::Other("Mock data")),
            );
        }

        is_valid(2 << 48);
        is_valid(-2 << 48);
        is_valid(0);
        is_valid(::std::i64::MAX);

        is_invalid("e");
        is_invalid("-0e");
        is_invalid("00e");
        is_invalid("05e");
        is_invalid(&format!("{}e", ::std::u64::MAX));
    }

    #[test]
    fn list() {
        assert(
            Benc::list,
            b"5:helloi42ee".bytes(),
            Ok(vec![B::String(bytes!("hello")), B::Int(42)]),
        );

        assert(
            Benc::list,
            b"5:helloi42eli2ei3e2:hid4:listli1ei2ei3ee7:yahallo2::)eed2:hi5:hello3:inti15eee"
                .bytes(),
            Ok(vec![
                B::String(bytes!("hello")),
                B::Int(42),
                B::List(vec![
                    B::Int(2),
                    B::Int(3),
                    B::String(bytes!("hi")),
                    B::Dict(hashmap!(
                        bytes!("list")    => B::List(vec!(B::Int(1), B::Int(2), B::Int(3))),
                        bytes!("yahallo") => B::String(bytes!(":)")),
                    )),
                ]),
                B::Dict(hashmap!(
                    bytes!("hi")  => B::String(bytes!("hello")),
                    bytes!("int") => B::Int(15),
                )),
            ]),
        );

        assert(
            Benc::list,
            b"5:helloi4e".bytes(),
            Err(error::Error::Other("Mock data")),
        );
    }

    #[test]
    fn dict() {
        assert(
            Benc::dict,
            b"2:hi5:helloe".bytes(),
            Ok(hashmap!(
                bytes!("hi") => B::String(bytes!("hello")),
            )),
        );

        assert(
            Benc::dict,
            concat!(
                "10:dictionaryd2:hi5:hello3:inti15ee7:integeri42e4:listli2ei3e2:hid4:listli1ei2e",
                "i3ee7:yahallo2::)ee3:str5:helloe"
            ).as_bytes()
                .bytes(),
            Ok(hashmap!(
                bytes!("str")     => B::String(bytes!("hello")),
                bytes!("integer") => B::Int(42),
                bytes!("list")    => B::List(vec![
                    B::Int(2),
                    B::Int(3),
                    B::String(bytes!("hi")),
                    B::Dict(hashmap!(
                        bytes!("list")    => B::List(vec![B::Int(1), B::Int(2), B::Int(3)]),
                        bytes!("yahallo") => B::String(bytes!(":)")),
                    )),
                ]),
                bytes!("dictionary") => B::Dict(hashmap!(
                    bytes!("hi")  => B::String(bytes!("hello")),
                    bytes!("int") => B::Int(15i64),
                )),
            )),
        );

        assert(
            Benc::dict,
            b"2:hi5:hello1:ai32ee".bytes(),
            Err(error::Error::Other(("Mock data"))),
        );
    }

    fn assert<R, O, E, F>(func: F, mut data: io::Bytes<R>, expect: Result<O, E>)
    where
        R: io::Read,
        O: PartialEq + Debug,
        E: PartialEq + Debug,
        F: Fn(&mut io::Bytes<R>) -> Result<O, E>,
    {
        let result = func(&mut data);

        match result {
            Ok(_) => assert!(result == expect, "{:?} == {:?}", result, expect),
            Err(_) => assert!(expect.is_err(), "{:?} == {:?}", result, expect),
        }
    }
}

#[cfg(feature = "bench")]
mod bench {
    extern crate test;

    use std::io::Read;

    use super::Benc;

    #[bench]
    fn new(b: &mut test::Bencher) {
        let data = concat!(
            "d8:announce40:http://tracker.example.com:8080/announce7:comment17:\"Hello mock data",
            "\"13:creation datei1234567890e9:httpseedsl31:http://direct.example.com/mock131:http:",
            "//direct.example.com/mock2e4:infod6:lengthi562949953421312e4:name15:あいえおう12:piece",
            " lengthi536870912eee").as_bytes();

        b.iter(|| Benc::new(&mut data.bytes()));
    }

    #[bench]
    fn string(b: &mut test::Bencher) {
        let data = "5:こんにちわ".as_bytes();

        b.iter(|| Benc::string(&mut data.bytes(), b'1'));
    }

    #[bench]
    fn int(b: &mut test::Bencher) {
        let s = format!("{}e", 2i64 << 48);
        let data = s.as_bytes();

        b.iter(|| Benc::int(&mut data.bytes()));
    }

    #[bench]
    fn list(b: &mut test::Bencher) {
        let data = concat!(
            "5:helloi42eli2ei3e2:hid4:listli1ei2ei3e",
            "e7:yahallo2::)eed2:hi5:hello3:inti15eee"
        ).as_bytes();

        b.iter(|| Benc::list(&mut data.bytes()));
    }

    #[bench]
    fn dict(b: &mut test::Bencher) {
        let data = concat!(
            "10:dictionaryd2:hi5:hello3:inti15ee7:",
            "integeri42e4:listli2ei3e2:hid4:listli",
            "1ei2ei3ee7:yahallo2::)ee3:str5:helloe"
        ).as_bytes();

        b.iter(|| Benc::dict(&mut data.bytes()));
    }
}
