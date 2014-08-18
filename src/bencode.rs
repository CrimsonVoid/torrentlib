use std::collections::hashmap::HashMap;
use std::io::{Chars, BufferedReader};

macro_rules! into_benc(
    ($node:expr) => (
        match $node {
            Ok(n)  => n.into_benc(),
            Err(e) => return Err(e),
        }
    );
)

#[deriving(PartialEq)]
enum NodeType {
    TString,
    TInt,
    TList,
    TDict,
}

impl NodeType {
    fn type_of(c: char) -> Option<NodeType> {
        match c {
            '0'..'9' => Some(TString),
            'i'      => Some(TInt),
            'l'      => Some(TList),
            'd'      => Some(TDict),
            _        => None,
        }
    }
}

#[deriving(Show, PartialEq)]
pub enum Benc {
    BString (String),
    BInt    (i64),
    BList   (Vec<Benc>),
    BDict   (HashMap<String, Benc>),
}

impl Benc {
    pub fn build_tree<T: Reader>(brd: &mut BufferedReader<T>) -> Result<Vec<Benc>, &'static str> {
        let mut ast = Vec::new();
        let mut it  = brd.chars();
        let err     = Err("Parse error");
    
        loop {
            let c = match it.next() {
                Some(Ok(c))  => c,
                Some(Err(e)) => return Err(e.desc),
                None         => return Ok(ast),
            };
    
            ast.push(match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(&mut it, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (&mut it)),
                Some(TList)   => into_benc!(Benc::benc_list(&mut it)),
                Some(TDict)   => into_benc!(Benc::benc_dict(&mut it)),
                None          => return err,
            });
        }
    }

    fn benc_string<B: Buffer>(chars: &mut Chars<B>, c: char) -> Result<String, &'static str> {
        // Valid - 5:hello
        let mut buf  = Vec::with_capacity(3);
        let mut last = '\0';
        let err      = Err("Invalid string bencoding");
    
        match c {
            '\0'     => (),
            '0'..'9' => buf.push(c as u8),
            _        => return err,
        }
    
        // Collect all numbers until ':'
        for c in chars {
            match c {
                Ok(c@'0'..'9') => buf.push(c as u8),
                Ok(c@':')      => { last = c; break; },
                Ok(_)          => return err,
                Err(e)         => return Err(e.desc),
            }
        }
    
        // Make sure we didn't exhuast `chars`
        if last != ':' || buf.len() == 0 {
            return err;
        }
        
        let mut len = unsafe {
            // We know `buf` is only valid ASCII characters
            let buf = buf.into_ascii_nocheck();
    
            match from_str::<uint>(buf.as_slice().as_str_ascii()) {
                Some(l) => l,
                None    => return err,
            }
        };
        let mut buf = String::with_capacity(len);

        // Read `len` chars
        for c in chars {
            match c {
                Ok(c)  => buf.push_char(c),
                Err(e) => return Err(e.desc),
            };
    
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

    fn benc_int<B: Buffer>(chars: &mut Chars<B>) -> Result<i64, &'static str> {
        // Valid   - i5e, i0e
        // Invalid - i05e, i00e ie
        let mut buf  = Vec::with_capacity(4);
        let mut last = '\0';
        let err      = Err("Invalid int bencoding");
    
        // Only the first char can be '-'
        match chars.next() {
            Some(Ok(c@'0'..'9')) => buf.push(c as u8),
            Some(Ok(c@'-'))      => buf.push(c as u8),
            Some(Ok(_))          => return err,
            Some(Err(e))         => return Err(e.desc),
            None                 => return err,
        }
    
        // Read numbers until 'e'
        for c in chars {
            match c {
                Ok(c@'0'..'9') => buf.push(c as u8),
                Ok(c@'e')      => { last = c; break; },
                Ok(_)          => return err,
                Err(e)         => return Err(e.desc),
            }
        }
    
        if last != 'e'         // Make sure we didn't exhuast `brd`
            || buf.len() == 0  // 'ie' is invalid
            || (buf[0] == '0' as u8 && buf.len() > 1)  // i05e is invalid
            || (buf.len() > 1 && buf.slice(0, 2) == b"-0")  // i-0e is invalid
            { return err; }
    
        unsafe {
            // We know `buf` is only valid ASCII characters
            let buf = buf.into_ascii_nocheck();
    
            match from_str(buf.as_slice().as_str_ascii()) {
                Some(n) => Ok(n),
                None    => err,
            }
        }
    }

    fn benc_list<B: Buffer>(chars: &mut Chars<B>) -> Result<Vec<Benc>, &'static str> {
        // Valid - l[Node]+e
        let mut list = Vec::new();
        let err      = Err("Invalid list bencoding");
    
        loop {
            let c = match chars.next() {
                Some(Ok('e')) => return Ok(list),
                Some(Ok(c))   => c,
                Some(Err(e))  => return Err(e.desc),
                None          => return Err("Unexpected end of file"),
            };
    
            list.push(match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(chars, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (chars)),
                Some(TList)   => into_benc!(Benc::benc_list(chars)),
                Some(TDict)   => into_benc!(Benc::benc_dict(chars)),
                None          => return err,
            });
        }
    }

    fn benc_dict<B: Buffer>(chars: &mut Chars<B>) -> Result<HashMap<String, Benc>, &'static str> {
        // Valid - d(<NString><Node>)+e
        let mut dict = HashMap::new();
        let mut key  = String::new();  // Previous key to make sure keys are in alphabetical order
        let err      = Err("Invalid dict bencoding");
        
        loop {
            // Key
            let mut c = match chars.next() {
                Some(Ok('e'))                                        => return Ok(dict),
                Some(Ok(c)) if NodeType::type_of(c) == Some(TString) => c,
                Some(Ok(_))                                          => return err,
                Some(Err(e))                                         => return Err(e.desc),
                None                                                 => return err,
            };
    
            key = match Benc::benc_string(chars, c) {
                Ok(k) => 
                    if key < k { k }
                    else       { return err },
                Err(e) => return Err(e),
            };
            let k = key.clone();
    
            // Value
            c = match chars.next() {
                Some(Ok(c))  => c,
                Some(Err(e)) => return Err(e.desc),
                None         => return err,
            };
    
            let exist = dict.insert(k, match NodeType::type_of(c) {
                Some(TString) => into_benc!(Benc::benc_string(chars, c)),
                Some(TInt)    => into_benc!(Benc::benc_int (chars)),
                Some(TList)   => into_benc!(Benc::benc_list(chars)),
                Some(TDict)   => into_benc!(Benc::benc_dict(chars)),
                None          => return err,
            });

            if !exist {
                return err;
            }
        }
    }
}

// TODO - decode and encode traits/implementations

trait IntoBenc {
    fn into_benc(self) -> Benc;
}

impl IntoBenc for String {
    fn into_benc(self) -> Benc { BString(self) }
}

impl IntoBenc for i64 {
    fn into_benc(self) -> Benc { BInt(self) }
}

impl IntoBenc for Vec<Benc> {
    fn into_benc(self) -> Benc { BList(self) }
}

impl IntoBenc for HashMap<String, Benc> {
    fn into_benc(self) -> Benc { BDict(self) }
}

#[cfg(test)]
mod tests {
    use std::fmt::Show;
    use std::io::{Chars, BufferedReader, MemReader};

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

    #[test]
    fn benc_string() {
        let is_valid = |data: &str, first: char| {
            let bind = |brd: &mut Chars<BufferedReader<MemReader>>| -> Result<String, &'static str> {
                Benc::benc_string(brd, first)
            };
            let expect = data.splitn(1, ':')
                .skip(1)
                .collect::<Vec<&str>>()
                .concat()
                .into_string();

            assert(bind, data, Ok(expect));
        };
        
        let is_invalid = |data: &str, first: char| {
            let bind = |brd: &mut Chars<BufferedReader<MemReader>>| -> Result<String, &'static str> {
                Benc::benc_string(brd, first)
            };

            assert(bind, data, Err("Mock data"));
        };

        is_valid("7:yahallo", '\0');
        is_valid("5:こんにちわ", '\0');
        is_valid(":\"hello\"", '7');
        is_valid("1:hellohello1", '1');
        is_valid("2:hi", '0');

        is_invalid("6:hello", '\0');
        is_invalid("5:hallo", 'a');
        is_invalid("", '\0');
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
        assert(Benc::benc_int, "e", Err("Mock data"));
        assert(Benc::benc_int, "-0e", Err("Mock data"));
        assert(Benc::benc_int, "00e", Err("Mock data"));
        assert(Benc::benc_int, "05e", Err("Mock data"));
    }

    #[test]
    fn benc_list() {
        assert(Benc::benc_list,
            "5:helloi42ee",
            Ok(vec!(
                BString(string!("hello")),
                BInt(42),
            ))
        );

        assert(Benc::benc_list,
            concat!("5:helloi42eli2ei3e2:hid4:listli1ei2ei3ee",
                    "7:yahallo2::)eed2:hi5:hello3:inti15eee"),
            Ok(vec!(
                BString(string!("hello")),
                BInt(42),
                BList(vec!(
                    BInt(2),
                    BInt(3),
                    BString(string!("hi")),
                    BDict(hashmap!(
                        string!("list")    => BList(vec!(BInt(1), BInt(2), BInt(3))),
                        string!("yahallo") => BString(string!(":)")),
                    )),
                )),
                BDict(hashmap!(
                    string!("hi")  => BString(string!("hello")),
                    string!("int") => BInt(15),
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
                string!("hi") => BString(string!("hello")),
            ))
        );

        assert(Benc::benc_dict,
            concat!("10:dictionaryd2:hi5:hello3:inti15ee",
                    "7:integeri42e4:listli2ei3e2:hid4:listli1ei2ei3e",
                    "e7:yahallo2::)ee3:str5:helloe"),
            Ok(hashmap!(
                string!("str")     => BString(string!("hello")),
                string!("integer") => BInt(42i64),
                string!("list")    => BList(vec!(
                    BInt(2),
                    BInt(3),
                    BString(string!("hi")),
                    BDict(hashmap!(
                        string!("list")    => BList(vec!(BInt(1), BInt(2), BInt(3))),
                        string!("yahallo") => BString(string!(":)")),
                    )),
                )),
                string!("dictionary") => BDict(hashmap!(
                    string!("hi")  => BString(string!("hello")),
                    string!("int") => BInt(15i64),
                )),
            ))
        );

        assert(Benc::benc_dict, "2hi:5:hello1:ai32e", Err("Mock data"));
    }

    fn assert<T: PartialEq+Show, E: PartialEq+Show>(
            func: |&mut Chars<BufferedReader<MemReader>>| -> Result<T, E>, 
            data: &str,
            expect: Result<T, E>) {
        let mut brd = BufferedReader::new(MemReader::new(data.to_string().into_bytes()));
        let result  = func(&mut brd.chars());

        match result {
            Ok(_)  => assert!(result == expect, "{} == {}", result, expect),
            Err(_) => assert!(expect.is_err(),  "{} == {}", result, expect),
        }
    }
}

