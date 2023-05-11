//@error-in-other-file: pointer to 1 byte starting at offset 9 is out-of-bounds
#![feature(portable_simd)]
use std::simd::*;

fn main() {
    unsafe {
        let mut vec: Vec<i8> = vec![10, 11, 12, 13, 14, 15, 16, 17, 18];
        let idxs = Simd::from_array([9, 3, 0, 17]);
        Simd::from_array([-27, 82, -41, 124]).scatter_select_unchecked(
            &mut vec,
            Mask::splat(true),
            idxs,
        );
    }
}
