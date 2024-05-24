//! Construction of MIR from HIR.
//!
//! This crate also contains the match exhaustiveness and usefulness checking.

#![allow(rustc::diagnostic_outside_of_impl)]
#![allow(rustc::untranslatable_diagnostic)]
#![feature(assert_matches)]
#![feature(box_patterns)]
#![feature(if_let_guard)]
#![feature(let_chains)]
#![feature(try_blocks)]

mod build;
mod check_unsafety;
mod errors;
pub mod lints;
mod thir;

use rustc_middle::util::Providers;

rustc_fluent_macro::fluent_messages! { "../messages.ftl" }

pub fn provide(providers: &mut Providers) {
    providers.check_match = thir::pattern::check_match;
    providers.lit_to_const = thir::constant::lit_to_const;
    providers.hooks.build_mir = build::mir_build;
    providers.closure_saved_names_of_captured_variables =
        build::closure_saved_names_of_captured_variables;
    providers.check_unsafety = check_unsafety::check_unsafety;
    providers.thir_body = thir::cx::thir_body;
    providers.hooks.thir_tree = thir::print::thir_tree;
    providers.hooks.thir_flat = thir::print::thir_flat;
}
