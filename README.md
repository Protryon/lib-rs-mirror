# Lib.rs (Crates.rs)

[Lib.rs](https://lib.rs) is a fast, lightweight way to browse information about all applications and libraries written in [Rust](https://www.rust-lang.org/).

Crates [published](https://doc.rust-lang.org/cargo/reference/publishing.html) to crates.io will automatically show up on lib.rs. Lib.rs is not a registry on its own, and it's not affiliated with crates.io or the Rust project.

## Building

 0. [Install Rust](https://www.rust-lang.org/install.html), [Node.js](https://nodejs.org/download/) (Node is used for [Sass styles](https://gitlab.com/crates.rs/style)), and [Docutils](http://docutils.sourceforge.net/) (for `.rst` readmes).

 1. Clone this repository **recursively**, so that all subprojects are included:

    ```sh
    git clone --recursive https://gitlab.com/crates.rs/crates.rs
    cd crates.rs
    ```

 2. Run `make`. It will download, compile and run everything. In case it doesn't work, try the step-by-step instructions below.

## Contributing

The site is open source. It aims to be friendly and will enforce [Code of Conduct](CODE_OF_CONDUCT.md) to Rust's high standards. Rust beginners are welcome. Contributions beyond just code, such as UX and design, are appreciated.

If you'd like to help improve it:

 * [See the list of open issues](https://gitlab.com/groups/crates.rs/-/issues) or [file an issue/bug report/question](https://gitlab.com/crates.rs/crates.rs/issues/new).

 * If you'd like to discuss the site or brainstorm solutions with a wider audience, [Rust user forum](http://users.rust-lang.org/) is a good place. You can [DM kornel](https://users.rust-lang.org/u/kornel), too.

### Where to find the code?

 * If you want to change look'n'feel (CSS): [see the `style` subproject](https://gitlab.com/crates.rs/style).
 * If you want to change HTML of the templates: [see the `front_end` dir](https://gitlab.com/crates.rs/crates.rs/tree/master/front_end).
 * If you want to show new kind of data on the pages:
     1. Fetch/compute that data in [one of the subprojects](https://gitlab.com/crates.rs) most relevant for that type of data (e.g. there's a subproject for [interacting with GitHub API](https://gitlab.com/crates.rs/crates.rs/tree/master/github_info) if you want to get information from there).
     2. Expose that data source [in the `kitchen_sink` dir](https://gitlab.com/crates.rs/crates.rs/tree/master/kitchen_sink) which connects all data sources together.
     3. Put that data in the page helper objects (e.g. `CratePage`) [in the `front_end` dir](https://gitlab.com/crates.rs/crates.rs/tree/master/front_end).
     4. Use the data in HTML templates.

## Building step-by-step

 1. [Get the initial data files](https://lib.rs/data/data.tar.xz) for the site (about 200MB).

 2. Extract the data files in `.xz` format using [7zip](https://www.7-zip.org/download.html), [The Unarchiver (Mac)](https://theunarchiver.com/) or `unxz data/*.xz`.
    * Put them all (`crate_data.db`, `cratesio.rmpz`, etc.) in the `data/` subdirectory of crates.rs checkout.

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

 4. Alternatively, start a local server:

     ```
     cd ../server
     cargo run
     ```

 5. Serve the HTML and the styles together:

    ```sh
    cd ../style
    npm start
    ```

    This will launch a local web server on [localhost:3000](http://localhost:3000) that serves HTML from `front_end/public/` and *live reload* styles from `style/src/*.scss`, so you can browse the site and edit the styles locally.

### Troubleshooting

* If you get "patch for … in … did not resolve to any crates." error when building, delete `Cargo.lock` files from the project.


