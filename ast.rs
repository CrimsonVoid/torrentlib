use std::collections::hashmap::HashMap;
use std::io::{Chars, BufferedReader};

macro_rules! into_node(
    ($node:expr, $err:expr) => (
        match $node {
            Some(n) => n.into_node(),
            None    => return $err,
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
pub enum Node {
    NString (String),
    NInt    (i64),
    NList   (Vec<Node>),
    NDict   (HashMap<String, Node>),
}

impl Node {
    pub fn build_tree<T: Reader>(brd: &mut BufferedReader<T>) -> Result<Vec<Node>, &str> {
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
                Some(TString) => into_node!(Node::benc_string(&mut it, c), err),
                Some(TInt)    => into_node!(Node::benc_int (&mut it), err),
                Some(TList)   => into_node!(Node::benc_list(&mut it), err),
                Some(TDict)   => into_node!(Node::benc_dict(&mut it), err),
                None          => return err,
            });
        }
    }

    fn benc_string<B: Buffer>(chars: &mut Chars<B>, c: char) -> Option<String> {
        // Valid - 5:hello
        let mut buf  = Vec::with_capacity(3);
        let mut last = '\0';
    
        match c {
            '\0'     => (),
            '0'..'9' => buf.push(c as u8),
            _        => return None,
        }
    
        // Collect all numbers until ':'
        for c in chars {
            match c {
                Ok(c@'0'..'9') => buf.push(c as u8),
                Ok(c@':')      => { last = c; break; },
                _              => return None,
            }
        }
    
        // Make sure we didn't exhuast `chars`
        if last != ':' || buf.len() == 0 {
            return None;
        }
        
        let mut len;
        unsafe {
            // We know `buf` is only valid ASCII characters
            let buf = buf.into_ascii_nocheck();
    
            len = match from_str::<uint>(buf.as_slice().as_str_ascii()) {
                Some(l) => l,
                None    => return None,
            };
        }
        let mut buf = String::with_capacity(len);
    
        // Read `len` chars
        for c in chars {
            match c {
                Ok(c)  => buf.push_char(c),
                Err(_) => return None,
            };
    
            len -= 1;
            if len == 0 {
                break;
            }
        }
    
        match len {
            0 => Some(buf),
            _ => None,
        }
    }

    fn benc_int<B: Buffer>(chars: &mut Chars<B>) -> Option<i64> {
        // Valid   - i5e, i0e
        // Invalid - i05e, i00e ie
        let mut buf  = Vec::with_capacity(4);
        let mut last = '\0';
    
        // Only the first char can be '-'
        match chars.next() {
            Some(Ok(c@'0'..'9')) => buf.push(c as u8),
            Some(Ok(c@'-'))      => buf.push(c as u8),
            _                    => return None,
        }
    
        // Read numbers until 'e'
        for c in chars {
            match c {
                Ok(c@'0'..'9') => buf.push(c as u8),
                Ok(c@'e')      => { last = c; break; },
                _              => return None,
            }
        }
    
        if last != 'e'         // Make sure we didn't exhuast `brd`
            || buf.len() == 0  // 'ie' is invalid
            || (buf[0] == '0' as u8 && buf.len() > 1)  // i05e is invalid
            || (buf.len() > 1 && buf.slice(0, 2) == b"-0")  // i-0e is invalid
            { return None; }
    
        unsafe {
            // We know `buf` is only valid ASCII characters
            let buf = buf.into_ascii_nocheck();
    
            from_str(buf.as_slice().as_str_ascii())
        }
    }

    fn benc_list<B: Buffer>(chars: &mut Chars<B>) -> Option<Vec<Node>> {
        // Valid - l[Node]+e
        let mut list = Vec::new();
    
        loop {
            let c = match chars.next() {
                Some(Ok('e')) => return Some(list),
                Some(Ok(c))   => c,
                Some(Err(_))  => return None,
                None          => return None,
            };
    
            list.push(match NodeType::type_of(c) {
                Some(TString) => into_node!(Node::benc_string(chars, c), None),
                Some(TInt)    => into_node!(Node::benc_int (chars), None),
                Some(TList)   => into_node!(Node::benc_list(chars), None),
                Some(TDict)   => into_node!(Node::benc_dict(chars), None),
                None          => return None
            });
        }
    }

    fn benc_dict<B: Buffer>(chars: &mut Chars<B>) -> Option<HashMap<String, Node>> {
        // Valid - d(<NString><Node>)+e
        let mut dict = HashMap::new();
        let mut key  = String::new();
        
        loop {
            // Key
            let mut c = match chars.next() {
                Some(Ok('e'))                                        => return Some(dict),
                Some(Ok(c)) if NodeType::type_of(c) == Some(TString) => c,
                _                                                    => return None,
            };
    
            key = match Node::benc_string(chars, c) {
                Some(k) => 
                    if key < k { k }
                    else       { return None },
                _ => return None,
            };
            let k = key.clone();
    
            // Value
            c = match chars.next() {
                Some(Ok(c)) => c,
                _           => return None,
            };
    
            let exist = dict.insert(k, match NodeType::type_of(c) {
                Some(TString) => into_node!(Node::benc_string(chars, c), None),
                Some(TInt)    => into_node!(Node::benc_int (chars), None),
                Some(TList)   => into_node!(Node::benc_list(chars), None),
                Some(TDict)   => into_node!(Node::benc_dict(chars), None),
                None          => return None,
            });

            if !exist {
                return None;
            }
        }
    }
}

trait IntoNode {
    fn into_node(self) -> Node;
}

impl IntoNode for String {
    fn into_node(self) -> Node { NString(self) }
}

impl IntoNode for i64 {
    fn into_node(self) -> Node { NInt(self) }
}

impl IntoNode for Vec<Node> {
    fn into_node(self) -> Node { NList(self) }
}

impl IntoNode for HashMap<String, Node> {
    fn into_node(self) -> Node { NDict(self) }
}

#[cfg(test)]
mod tests {
    use std::fmt::Show;
    use std::io::{Chars, BufferedReader, MemReader};

    use super::{NString, NInt, NList, NDict};
    use super::Node;

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

    macro_rules! valid(
        ($func:expr, $data:expr, $expect:expr) => (
            validty($func, $data, Some($expect));
        );

        ($func:expr, $data:expr) => (
            validty($func, $data, None);
        );
    )

    #[test]
    fn benc_string() {
        let is_valid = |data: &str, first: char| {
            let bind = |brd: &mut Chars<BufferedReader<MemReader>>| -> Option<String> {
                Node::benc_string(brd, first)
            };
            let expect = data.splitn(':', 1)
                .skip(1)
                .collect::<Vec<&str>>()
                .concat()
                .into_string();

            valid!(bind, data, expect);
        };
        
        let is_invalid = |data: &str, first: char| {
            let bind = |brd: &mut Chars<BufferedReader<MemReader>>| -> Option<String> {
                Node::benc_string(brd, first)
            };

            valid!(bind, data);
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
            valid!(Node::benc_int, format!("{}e", expect).as_slice(), expect);
        };

        // Valid
        is_valid(2<<48);
        is_valid(-2<<48);
        is_valid(0);

        // Invalid
        valid!(Node::benc_int, "e");
        valid!(Node::benc_int, "-0e");
        valid!(Node::benc_int, "00e");
        valid!(Node::benc_int, "05e");
    }

    #[test]
    fn benc_list() {
        valid!(Node::benc_list,
            "5:helloi42ee",
            vec!(
                NString(string!("hello")),
                NInt(42),
            )
        );

        valid!(Node::benc_list,
            concat!("5:helloi42eli2ei3e2:hid4:listli1ei2ei3ee",
                    "7:yahallo2::)eed2:hi5:hello3:inti15eee"),
            vec!(
                NString(string!("hello")),
                NInt(42),
                NList(vec!(
                    NInt(2),
                    NInt(3),
                    NString(string!("hi")),
                    NDict(hashmap!(
                        string!("list")    => NList(vec!(NInt(1), NInt(2), NInt(3))),
                        string!("yahallo") => NString(string!(":)")),
                    )),
                )),
                NDict(hashmap!(
                    string!("hi")  => NString(string!("hello")),
                    string!("int") => NInt(15),
                )),
            )
        );

        valid!(Node::benc_list, "5:helloi4e");
    }

    #[test]
    fn benc_dict() {
        valid!(Node::benc_dict,
            "2:hi5:helloe",
            hashmap!(
                string!("hi") => NString(string!("hello")),
            )
        );

        valid!(Node::benc_dict,
            concat!("10:dictionaryd2:hi5:hello3:inti15ee",
                    "7:integeri42e4:listli2ei3e2:hid4:listli1ei2ei3e",
                    "e7:yahallo2::)ee3:str5:helloe"),
            hashmap!(
                string!("str")     => NString(string!("hello")),
                string!("integer") => NInt(42i64),
                string!("list")    => NList(vec!(
                    NInt(2),
                    NInt(3),
                    NString(string!("hi")),
                    NDict(hashmap!(
                        string!("list")    => NList(vec!(NInt(1), NInt(2), NInt(3))),
                        string!("yahallo") => NString(string!(":)")),
                    )),
                )),
                string!("dictionary") => NDict(hashmap!(
                    string!("hi")  => NString(string!("hello")),
                    string!("int") => NInt(15i64),
                )),
            )
        );

        valid!(Node::benc_dict, "2hi:5:hello1:ai32e");
    }

    fn validty<T: PartialEq+Show>(func: |&mut Chars<BufferedReader<MemReader>>| -> Option<T>,
                                data: &str,
                                expect: Option<T>) {
        let mut brd = BufferedReader::new(
            MemReader::new(String::from_str(data).into_bytes()));
        let result = func(&mut brd.chars());

        assert!(result == expect, "{} == {}", result, expect);
    }
}
