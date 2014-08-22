//! Decode and encode bencoded values as described by [BEP 003](
//! http://www.bittorrent.org/beps/bep_0003.html).

use std::collections::hashmap::HashMap;
use std::io::extensions::Bytes;

macro_rules! into_benc(
    ($node:expr) => (
        match $node {
            Ok(n)  => n.into_benc(),
            Err(e) => return Err(e),
        }
    );
)

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
            b'0'..b'9' => Some(TString),
            b'i'       => Some(TInt),
            b'l'       => Some(TList),
            b'd'       => Some(TDict),
            _          => None,
        }
    }
}

/// The types that can be represented as a bencoded values
#[deriving(Show, PartialEq)]
pub enum Benc {
    BString (Vec<u8>),
    BInt    (i64),
    BList   (Vec<Benc>),
    BDict   (HashMap<Vec<u8>, Benc>),
}

impl Benc {
    /// Consumes the Reader and builds a Vec of `Benc` values. The function will return early if
    /// an invalid Benc node is found.
    pub fn build_tree<R: Reader>(bytes: &mut Bytes<R>) -> Result<Vec<Benc>, &'static str> {
        let mut ast = Vec::new();
        let err     = Err("Parse error");
    
        loop {
            let c = match bytes.next() {
                // TODO - Handle '\n'?
                Some(Ok(b'\n')) => continue,
                Some(Ok(b'\0')) => return Ok(ast),
                Some(Ok(c))     => c,
                Some(Err(e))    => return Err(e.desc),
                None            => return Ok(ast),
            };
    
            ast.push(match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(bytes, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (bytes)),
                Some(TList)   => into_benc!(Benc::benc_list(bytes)),
                Some(TDict)   => into_benc!(Benc::benc_dict(bytes)),
                None          => return err,
            });
        }
    }

    /// Consumes as much of the Buffer as needed to read a valid bencoded string. `c` is the first
    /// bencoded char of the string, which can by '\0'
    fn benc_string<R: Reader>(bytes: &mut Bytes<R>, c: u8) -> Result<Vec<u8>, &'static str> {
        // Valid - 5:hello
        let mut buf  = Vec::with_capacity(3);
        let mut last = b'\0';
        let err      = Err("Invalid string bencoding");
    
        match c {
            b'\0'      => (),
            b'0'..b'9' => buf.push(c.to_ascii()),
            _          => return err,
        }
    
        // Collect all numbers until ':'
        for c in bytes {
            match c {
                Ok(c@b'0'..b'9') => buf.push(c.to_ascii()),
                Ok(c@b':')       => { last = c; break; },
                Ok(_)            => return err,
                Err(e)           => return Err(e.desc),
            }
        }

        // Make sure we didn't exhuast `chars`
        if last != b':' || buf.len() == 0 {
            return err;
        }

        let mut len = match from_str(buf.as_slice().as_str_ascii()) {
                Some(l) => l,
                None    => return err,
        };
        let mut buf = Vec::with_capacity(len);

        // Read `len` bytes
        for c in bytes {
            match c {
                Ok(c)  => buf.push(c),
                Err(e) => return Err(e.desc),
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

    /// Consumes as much of the Buffer as needed to read a valid bencoded int
    fn benc_int<R: Reader>(bytes: &mut Bytes<R>) -> Result<i64, &'static str> {
        // Valid   - i5e, i0e
        // Invalid - i05e, i00e ie
        let mut buf  = Vec::with_capacity(4);
        let mut last = b'\0';
        let err      = Err("Invalid int bencoding");
    
        // Only the first char can be '-'
        buf.push(match bytes.next() {
            // Known to be valid ASCII
            Some(Ok(c@b'0'..b'9')) => c.to_ascii(),
            Some(Ok(c@b'-'))       => c.to_ascii(),
            Some(Ok(_))            => return err,
            Some(Err(e))           => return Err(e.desc),
            None                   => return err,
        });
    
        // Read numbers until 'e'
        for c in bytes {
            match c {
                Ok(c@b'0'..b'9') => buf.push(c.to_ascii()),
                Ok(c@b'e')       => { last = c; break; },
                Ok(_)            => return err,
                Err(e)           => return Err(e.desc),
            }
        }
    
        if last != b'e'        // Make sure we didn't exhuast `bytes`
            || buf.len() == 0  // 'ie' is invalid
            || (buf[0] == '0'.to_ascii() && buf.len() > 1)           // i05e is invalid
            || (buf.len() > 1 && buf.slice(0, 2) == "-0".to_ascii()) // i-0e is invalid
            { return err; }
    
        match from_str(buf.as_slice().as_str_ascii()) {
            Some(n) => Ok(n),
            None    => err,
        }
    }

    /// Consumes as much of the Buffer as needed to read a valid bencoded list
    fn benc_list<R: Reader>(bytes: &mut Bytes<R>) -> Result<Vec<Benc>, &'static str> {
        // Valid - l[Node]+e
        let mut list = Vec::new();
        let err      = Err("Invalid list bencoding");
    
        loop {
            let c = match bytes.next() {
                Some(Ok(b'e')) => return Ok(list),
                Some(Ok(c))    => c,
                Some(Err(e))   => return Err(e.desc),
                None           => return Err("Unexpected end of file"),
            };
    
            list.push(match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(bytes, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (bytes)),
                Some(TList)   => into_benc!(Benc::benc_list(bytes)),
                Some(TDict)   => into_benc!(Benc::benc_dict(bytes)),
                None          => return err,
            });
        }
    }

    /// Consumes as much of the Buffer as needed to read a valid bencoded dictionary. Dictionary
    /// keys are `BString`s
    fn benc_dict<R: Reader>(bytes: &mut Bytes<R>) -> Result<HashMap<Vec<u8>, Benc>, &'static str> {
        // Valid - d(<NString><Node>)+e
        let mut dict = HashMap::new();
        let mut key  = Vec::new();  // Previous key; ensure keys are in alphabetical order
        let err      = Err("Invalid dict bencoding");
        
        loop {
            // Key
            let mut c = match bytes.next() {
                Some(Ok(b'e'))                                       => return Ok(dict),
                Some(Ok(c)) if NodeType::type_of(c) == Some(TString) => c,
                Some(Ok(_))                                          => return err,
                Some(Err(e))                                         => return Err(e.desc),
                None                                                 => return err,
            };
    
            key = match Benc::benc_string(bytes, c) {
                Ok(k)  => if key < k { k } else { return err },
                Err(e) => return Err(e),
            };
            let k = key.clone();
    
            // Value
            c = match bytes.next() {
                Some(Ok(c))  => c,
                Some(Err(e)) => return Err(e.desc),
                None         => return err,
            };
    
            let exist = dict.insert(k, match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(bytes, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (bytes)),
                Some(TList)   => into_benc!(Benc::benc_list(bytes)),
                Some(TDict)   => into_benc!(Benc::benc_dict(bytes)),
                None          => return err,
            });

            if !exist {
                return err;
            }
        }
    }
}

// TODO - decode and encode traits/implementations

/// Trait to consume the value returning a `Benc` type
trait IntoBenc {
    fn into_benc(self) -> Benc;
}

impl IntoBenc for String {
    fn into_benc(self) -> Benc { BString(self.into_bytes()) }
}

impl IntoBenc for Vec<u8> {
    fn into_benc(self) -> Benc{ BString(self) }
}

impl IntoBenc for i64 {
    fn into_benc(self) -> Benc { BInt(self) }
}

impl IntoBenc for Vec<Benc> {
    fn into_benc(self) -> Benc { BList(self) }
}

impl IntoBenc for HashMap<Vec<u8>, Benc> {
    fn into_benc(self) -> Benc { BDict(self) }
}

#[cfg(test)]
mod tests {
    use std::fmt::Show;
    use std::io::extensions::Bytes;
    use std::io::MemReader;

    use super::Benc;
    use super::{BString, BInt, BList, BDict};

    macro_rules! hashmap(
        ($($k:expr => $v:expr),*) => ({
            let mut d = ::std::collections::hashmap::HashMap::new();
            $(d.insert($k, $v);)*
            d
        });
    
        ($($k:expr => $v:expr),+,) => (hashmap!($($k => $v),+));
    )

    macro_rules! string(
        ($s:expr) => (
            $s.into_string()
        );
    )
    
    macro_rules! bytes(
        ($s:expr) => (
            $s.into_string().into_bytes()
        );
    )

    #[test]
    fn benc_string() {
        let is_valid = |data: &str, first: u8| {
            let bind = |brd: &mut Bytes<MemReader>| -> Result<Vec<u8>, &'static str> {
                Benc::benc_string(brd, first)
            };
            let expect = data.splitn(1, ':')
                .skip(1)
                .collect::<Vec<&str>>()
                .concat()
                .into_string();

            assert(bind, data, Ok(bytes!(expect)));
        };
        
        let is_invalid = |data: &str, first: u8| {
            let bind = |brd: &mut Bytes<MemReader>| -> Result<Vec<u8>, &'static str> {
                Benc::benc_string(brd, first)
            };

            assert(bind, data, Err("Mock data"));
        };

        is_valid("7:yahallo", b'\0');
        is_valid("15:こんにちわ", b'\0'); // Bytes, not chars
        is_valid(":\"hello\"", b'7');
        is_valid("1:hellohello1", b'1');
        is_valid("2:hi", b'0');

        is_invalid("6:hello", b'\0');
        is_invalid("5:hallo", b'a');
        is_invalid("", b'\0');
    }

    #[test]
    fn benc_int() {
        let is_valid = |expect: i64| {
            assert(Benc::benc_int, format!("{}e", expect).as_slice(), Ok(expect));
        };

        // Valid
        is_valid(2<<48);
        is_valid(-2<<48);
        is_valid(0);

        // Invalid
        assert(Benc::benc_int, "e",   Err("Mock data"));
        assert(Benc::benc_int, "-0e", Err("Mock data"));
        assert(Benc::benc_int, "00e", Err("Mock data"));
        assert(Benc::benc_int, "05e", Err("Mock data"));
    }

    #[test]
    fn benc_list() {
        assert(Benc::benc_list,
            "5:helloi42ee",
            Ok(vec!(
                BString(bytes!("hello")),
                BInt(42),
            ))
        );

        assert(Benc::benc_list,
            concat!("5:helloi42eli2ei3e2:hid4:listli1ei2ei3ee",
                    "7:yahallo2::)eed2:hi5:hello3:inti15eee"),
            Ok(vec!(
                BString(bytes!("hello")),
                BInt(42),
                BList(vec!(
                    BInt(2),
                    BInt(3),
                    BString(bytes!("hi")),
                    BDict(hashmap!(
                        bytes!("list")    => BList(vec!(BInt(1), BInt(2), BInt(3))),
                        bytes!("yahallo") => BString(bytes!(":)")),
                    )),
                )),
                BDict(hashmap!(
                    bytes!("hi")  => BString(bytes!("hello")),
                    bytes!("int") => BInt(15),
                )),
            ))
        );

        assert(Benc::benc_list, "5:helloi4e", Err("Mock data"));
    }

    #[test]
    fn benc_dict() {
        assert(Benc::benc_dict,
            "2:hi5:helloe",
            Ok(hashmap!(
                bytes!("hi") => BString(bytes!("hello")),
            ))
        );

        assert(Benc::benc_dict,
            concat!("10:dictionaryd2:hi5:hello3:inti15ee",
                    "7:integeri42e4:listli2ei3e2:hid4:listli1ei2ei3e",
                    "e7:yahallo2::)ee3:str5:helloe"),
            Ok(hashmap!(
                bytes!("str")     => BString(bytes!("hello")),
                bytes!("integer") => BInt(42),
                bytes!("list")    => BList(vec!(
                    BInt(2),
                    BInt(3),
                    BString(bytes!("hi")),
                    BDict(hashmap!(
                        bytes!("list")    => BList(vec!(BInt(1), BInt(2), BInt(3))),
                        bytes!("yahallo") => BString(bytes!(":)")),
                    )),
                )),
                bytes!("dictionary") => BDict(hashmap!(
                    bytes!("hi")  => BString(bytes!("hello")),
                    bytes!("int") => BInt(15i64),
                )),
            ))
        );

        assert(Benc::benc_dict, "2hi:5:hello1:ai32e", Err("Mock data"));
    }

    fn assert<T: PartialEq+Show, E: PartialEq+Show>(
            func: |&mut Bytes<MemReader>| -> Result<T, E>, 
            data: &str,
            expect: Result<T, E>) {
        let mut brd = MemReader::new(data.to_string().into_bytes());
        let result  = func(&mut brd.bytes());

        match result {
            Ok(_)  => assert!(result == expect, "{} == {}", result, expect),
            Err(_) => assert!(expect.is_err(),  "{} == {}", result, expect),
        }
    }
}

