//@ compile-flags: -O -C lto=thin -C prefer-dynamic=no
//@ only-x86_64-unknown-linux-gnu
//@ assembly-output: emit-asm

// CHECK: main

pub fn main() {
}
