use std::io::MemReader;

use torrent::bencode::Benc;
use torrent::bencode::{BString, BInt, BList, BDict};

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
fn build_tree() {
    let data = concat!("d8:announce40:http://tracker.example.com:8080/ann",
        "ounce7:comment17:\"Hello mock data\"13:creation datei1234567890e",
        "9:httpseedsl31:http://direct.example.com/mock131:http://direct.e",
        "xample.com/mock2e4:infod6:lengthi562949953421312e4:name15:あいえ",
        "おう12:piece lengthi536870912eee");
    
    let expect = vec!(BDict(hashmap!(
        string!("announce")      => BString(string!("http://tracker.example.com:8080/announce")),
        string!("comment")       => BString(string!("\"Hello mock data\"")),
        string!("creation date") => BInt(1234567890),
        string!("httpseeds")     => BList(vec!(
            BString(string!("http://direct.example.com/mock1")),
            BString(string!("http://direct.example.com/mock2")),
        )),
        string!("info") => BDict(hashmap!(
            string!("length")       => BInt(562949953421312),
            string!("name")         => BString(string!("あいえおう")),
            string!("piece length") => BInt(536870912),
        )),
    )));

    let mut brd = MemReader::new(data.to_string().into_bytes());

    let expect = Ok(expect);
    let result = Benc::build_tree(&mut brd.bytes());

    assert!(result == expect, "{} == {}", result, expect);
}

