use std::io::prelude::*;
use flate2::Compression;
use flate2::write::DeflateEncoder;
use flate2::read::DeflateDecoder;
use std::fmt;

/// gzip-compressed string
#[derive(Clone, Eq, PartialEq, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct ComprString(Box<[u8]>);

impl ComprString {
    pub fn new(s: &str) -> Self {
        let mut e = DeflateEncoder::new(Vec::with_capacity(s.len()/2), Compression::best());
        e.write_all(s.as_bytes()).unwrap();
        ComprString(e.finish().unwrap().into_boxed_slice())
    }

    pub fn to_string(&self) -> String {
        let mut deflater = DeflateDecoder::new(&self.0[..]);
        let mut s = String::with_capacity(self.0.len()*2);
        deflater.read_to_string(&mut s).unwrap();
        s
    }

    pub fn compressed_len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Display for ComprString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

impl fmt::Debug for ComprString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.to_string().fmt(f)
    }
}

impl From<String> for ComprString {
    fn from(o: String) -> Self {
        Self::new(&o)
    }
}

impl Into<String> for ComprString {
    fn into(self) -> String {
        self.to_string()
    }
}

impl<'a> From<&'a str> for ComprString {
    fn from(o: &'a str) -> Self {
        Self::new(o)
    }
}

#[test]
fn test() {
    let s = ComprString::new("hęllo world");
    assert_eq!("hęllo world", &s.to_string());
    assert_eq!("hęllo world", &format!("{}", s));

    let s = ComprString::new("");
    assert_eq!("", &s.to_string());
    assert_eq!(2, s.compressed_len());

    let l = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let s = ComprString::new(l);
    assert_eq!(l, &s.to_string());
    assert!(s.compressed_len() < l.len());
}
