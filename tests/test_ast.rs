use std::io::{BufferedReader, MemReader};

use torrent::ast::{NString, NInt, NList, NDict};
use torrent::ast::Node;

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
        "xample.com/mock2e4:infod6:lengthi562949953421312e4:name9:mock.da",
        "ta12:piece lengthi536870912eee");
    
    let expect = vec!(NDict(hashmap!(
        string!("announce")      => NString(string!("http://tracker.example.com:8080/announce")),
        string!("comment")       => NString(string!("\"Hello mock data\"")),
        string!("creation date") => NInt(1234567890),
        string!("httpseeds")     => NList(vec!(
            NString(string!("http://direct.example.com/mock1")),
            NString(string!("http://direct.example.com/mock2")),
        )),
        string!("info") => NDict(hashmap!(
            string!("length")       => NInt(562949953421312),
            string!("name")         => NString(string!("mock.data")),
            string!("piece length") => NInt(536870912),
        )),
    )));

    let mut brd = BufferedReader::new(
        MemReader::new(String::from_str(data).into_bytes()));

    let expect = Ok(expect);
    let result = Node::build_tree(&mut brd);

    println!("{}", result);
    assert!(result == expect, "{} == {}", result, expect);
}
