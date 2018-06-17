use std::io;
use cargo_toml;

quick_error! {
    #[derive(Debug, Clone)]
    pub enum UnarchiverError {
        GitCheckoutFailed(err: String) {
            display("Git checkout failed: {}", err)
        }
        TomlNotFound(files: String) {
            display("Cargo.toml not found\nFound files: {}", files)
        }
        TomlParse(err: cargo_toml::Error) {
            display("Cargo.toml parsing error: {}", err)
            from()
        }
        Io(err: String) {
            from(err: io::Error) -> (err.to_string())
        }
    }
}
