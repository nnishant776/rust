// When using the --test flag with an rlib, this used to generate
// an unwanted .bc file, which should not exist. This test checks
// that the bug causing the generation of this file has not returned.
// See https://github.com/rust-lang/rust/issues/10973

//@ ignore-cross-compile

use run_make_support::{rustc, tmp_dir};
use std::fs;

fn main() {
    rustc().crate_type("rlib").arg("--test").input("foo.rs").run();
    assert!(
        fs::remove_file(tmp_dir().join("foo.bc")).is_err(),
        "An unwanted .bc file was created by run-make/no-intermediate-extras."
    );
}
