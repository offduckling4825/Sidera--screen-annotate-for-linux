// Sidera - Rust migration command-line entry point
// Copyright (C) 2026 Carl_Jin
// SPDX-License-Identifier: GPL-3.0-only

#![forbid(unsafe_code)]

use sidera_bridge::{Bridge, BridgeConfig, DEFAULT_BIND_ADDRESS, DEFAULT_PORT};
use std::io;
use std::net::TcpListener;

fn print_help() {
    println!(
        "Sidera Rust migration CLI\n\n\
         Usage:\n  sidera <COMMAND> [OPTIONS]\n\n\
         Commands:\n  serve       Start the local WPS HTTP bridge\n  diagnose    Print runtime/platform information\n  help        Show this help\n\n\
         serve options:\n  --port N    Listen on 127.0.0.1:N (default: 16666)\n\n\
         This first Rust milestone contains the platform-independent core and\n\
         WPS bridge. The X11 overlay GUI is intentionally still provided by\n\
         the original C++ executable until a later migration milestone."
    );
}

fn parse_port(args: &[String]) -> Result<u16, String> {
    let mut port = DEFAULT_PORT;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--port requires a value".to_owned())?;
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid port: {value}"))?;
            }
            option => return Err(format!("unknown serve option: {option}")),
        }
        index += 1;
    }
    Ok(port)
}

fn diagnose() {
    let bridge = Bridge::new(BridgeConfig::default());
    println!("Sidera Rust runtime diagnosis");
    println!(
        "  session: {}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_owned())
    );
    println!(
        "  display: {}",
        std::env::var("DISPLAY").unwrap_or_else(|_| "unset".to_owned())
    );
    println!(
        "  wayland_display: {}",
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "unset".to_owned())
    );
    println!(
        "  addin_dir: {}",
        bridge
            .resolved_addin_dir()
            .map_or_else(|| "not found".to_owned(), |path| path.display().to_string())
    );
    println!("  bridge: http://{}:{}", DEFAULT_BIND_ADDRESS, DEFAULT_PORT);
    println!("  gui: original C++/Qt5 X11 implementation");
}

fn serve(port: u16) -> io::Result<()> {
    let bridge = Bridge::new(BridgeConfig::default());
    let listener = TcpListener::bind((DEFAULT_BIND_ADDRESS, port))?;
    println!("Sidera Rust bridge listening on http://{DEFAULT_BIND_ADDRESS}:{port}");
    if let Some(path) = bridge.resolved_addin_dir() {
        println!("Serving WPS add-in files from {}", path.display());
    } else {
        println!("WPS add-in directory not found; API endpoints remain available");
    }
    bridge.serve(listener)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    let rest: Vec<String> = args.collect();
    let result = match command.as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" => {
            println!("sidera {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "diagnose" => {
            if !rest.is_empty() {
                Err("diagnose does not accept options".to_owned())
            } else {
                diagnose();
                Ok(())
            }
        }
        "serve" => {
            parse_port(&rest).and_then(|port| serve(port).map_err(|error| error.to_string()))
        }
        other => Err(format!("unknown command: {other}")),
    };

    if let Err(error) = result {
        eprintln!("sidera: {error}");
        eprintln!("run `sidera --help` for usage");
        std::process::exit(2);
    }
}
