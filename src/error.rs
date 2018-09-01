use std::io;
use cargo_toml;

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
