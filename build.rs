use std::env;
use std::process::Command;

fn main() {
    // Local dev sets this (Makefile's `start` target) so cargo doesn't rebuild
    // the front-end while bun's dev server is already serving it.
    if env::var("SKIP_CLIENT_BUILD").is_ok() {
        return;
    }

    println!("cargo:rerun-if-changed=src/client");

    let output = Command::new("bun")
        .arg("run")
        .arg("build")
        .output()
        .expect("Failed to execute bun command.");

    if !output.status.success() {
        panic!("Building client failed");
    }
}
