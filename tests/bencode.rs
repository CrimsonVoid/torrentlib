use std::io::MemReader;
use std::io::extensions::Bytes;

use libbittorrent::bencode::Benc;
use libbittorrent::bencode::Benc as B;

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
fn build_tree() {
    let data = concat!("d8:announce40:http://tracker.example.com:8080/announce7:comment17:\"Hell",
        "o mock data\"13:creation datei1234567890e9:httpseedsl31:http://direct.example.com/mock1",
        "31:http://direct.example.com/mock2e4:infod6:lengthi562949953421312e4:name15:あいえおう1",
        "2:piece lengthi536870912eee");
    
    let expect = vec!(B::Dict(hashmap!(
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
    )));

    let mut brd = MemReader::new(bytes!(data));

    let expect = Ok(expect);
    let result = Benc::build_tree(&mut Bytes::new(&mut brd));

    assert!(result == expect, "{} == {}", result, expect);
}

