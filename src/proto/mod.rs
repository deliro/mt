#![allow(clippy::all)]
#![allow(unused_imports)]
#![allow(non_snake_case)]
#![allow(rustdoc::invalid_html_tags)]

pub mod meshtastic {
    include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
}

pub mod port;
