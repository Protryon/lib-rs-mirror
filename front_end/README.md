# HTML of crates.rs

This is a static site generator and [ructe](https://crates.rs/crates/ructe) templates for the crates.rs website.

## Usage

See [installation instructions for the crates.rs project](https://gitlab.com/crates.rs/crates.rs).

## Changing CSS

Styles are in a [separate project](https://gitlab.com/crates.rs/style) and need to be built before you compile this project.

## Adding templates

All `templates/*.rs.html` files are automatically included as templates in `front_end.rs`. [Documentation for the template syntax](https://docs.rs/ructe/0.4.0/ructe/Template_syntax/index.html).
