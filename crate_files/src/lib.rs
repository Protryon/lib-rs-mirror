#[macro_use] extern crate quick_error;
pub type Result<T> = std::result::Result<T, UnarchiverError>;
use libflate::gzip::Decoder;
use std::io::Read;
use std::io;
use tar::{Archive, Entries, Entry};

quick_error! {
    #[derive(Debug, Clone)]
    pub enum UnarchiverError {
        TomlNotFound(files: String) {
            display("Cargo.toml not found\nFound files: {}", files)
        }
        TomlParse(err: cargo_toml::Error) {
            display("Cargo.toml parsing error: {}", err)
            from()
            cause(err)
        }
        Io(err: String) {
            from(err: io::Error) -> (err.to_string())
        }
    }
}

/// Self-referential struct.
/// This can't implement iterator itself.
pub struct Files<'r, R: Read + 'r> {
    leaked_archive: *mut Archive<R>,
    pinned_entries: Option<Entries<'r, R>>,
}

impl<'r, R: Read + 'r> Drop for Files<'r, R> {
    fn drop(&mut self) {
        self.pinned_entries.take();
        unsafe { Box::from_raw(self.leaked_archive) };
    }
}

impl<'r, 'i, R: Read + 'r> IntoIterator for &'i mut Files<'r, R> {
    type IntoIter = &'i mut Entries<'i, R>;
    type Item = std::io::Result<Entry<'i, R>>;

    fn into_iter(self) -> Self::IntoIter {
        let entries = self.pinned_entries.as_mut().unwrap();
        // shorten lifetime of entries to lifetime of the `Files`
        unsafe { std::mem::transmute::<&'i mut Entries<'r, R>, &'i mut Entries<'i, R>>(entries) }
    }
}

pub fn read_archive_files<'a>(archive: impl Read) -> Result<Files<'a, impl Read>> {
    let archive = Box::new(Archive::new(Decoder::new(archive)?)); // Box gives it stable addr
    let archive = Box::into_raw(archive);
    let entries = unsafe{(archive.as_mut().unwrap())}.entries()?;
    Ok(Files {
        leaked_archive: archive,
        pinned_entries: Some(entries),
    })
}
