// run-pass

#![feature(coroutines, coroutine_trait)]

use std::ops::Coroutine;
use std::pin::Pin;

fn main() {
    let b = |_| 3;
    let mut a = || {
        b(yield);
    };
    Pin::new(&mut a).resume(());
}
