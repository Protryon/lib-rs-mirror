# Crates.rs

[Crates.rs](https://crates.rs) is a fast, lightweight way to browse information about all applications and libraries written in [Rust](https://www.rust-lang.org/).

Crates [published](https://doc.rust-lang.org/cargo/reference/publishing.html) to crates.**io** will automatically show up on crates.**rs**. Crates.rs is not a registry on its own, and it's not affiliated with crates.io or the Rust project.

## Contributing

The site is open source. It aims to be friendly and will enforce [Code of Conduct](CODE_OF_CONDUCT.md) to Rust's high standards. Rust begginers are welcome. Contributions beyond just code, such as UX and design, are appreciated.

If you'd like to help improve it:

 * [See the list of open issues](https://gitlab.com/groups/crates.rs/-/issues) or [file an issue/bug report/question](https://gitlab.com/crates.rs/crates.rs/issues/new).

 * If you'd like to discuss the site or brainstorm solutions with a wider audience, [Rust user forum](http://users.rust-lang.org/) is a good place. You can [DM kornel](https://users.rust-lang.org/u/kornel), too.

## Building

 0. [Install Rust](https://www.rust-lang.org/install.html) and [Node.js](https://nodejs.org/download/) (Node is used for [Sass styles](https://gitlab.com/crates.rs/style))

 1. Clone this repository **recursively**, so that all subprojects are included:

    ```sh
    git clone --recursive https://gitlab.com/crates.rs/crates.rs
    cd crates.rs
    ```

 2. [Get the initial data files](https://crates.rs/data) for the site (about 2GB).
    * Extract files in `.xz` format using [7zip](https://www.7-zip.org/download.html), [The Unarchiver (Mac)](https://theunarchiver.com/) or `unxz data/*.xz`.
    * Put them all (`cache.db`, `crates.db`, `github.db`, `users.db`, `category_keywords.db`) in the `data/` subdirectory of crates.rs checkout.

 3. Generate front-end [styles](https://gitlab.com/crates.rs/style):

    ```sh
    cd style
    npm install
    npm run build
    ```

 4. Generate HTML (this may take a few minutes):

    ```sh
    cd ../front_end
    cargo run --release --bin website
    ```

    If all goes well, this will create about 7000 HTML files in the `front_end/public/` directory.

 5. Serve the HTML and the styles together:

    ```sh
    cd ../style
    npm start
    ```

    This will launch a local web server on [localhost:3000](http://localhost:3000) that serves HTML from `front_end/public/` and *live reload* styles from `style/src/*.scss`, so you can browse the site and edit the styles locally.
