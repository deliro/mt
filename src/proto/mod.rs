#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction,
    non_snake_case,
    dead_code,
    unused_imports,
    rustdoc::invalid_html_tags,
    rustdoc::broken_intra_doc_links,
    rustdoc::invalid_codeblock_attributes,
    rustdoc::bare_urls,
    rustdoc::invalid_rust_codeblocks
)]
pub mod meshtastic {
    include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
}

pub mod port;
