// skip-filecheck
#![feature(custom_mir, core_intrinsics)]

extern crate core;
use core::intrinsics::mir::*;

// EMIT_MIR debuginfo.pointee.built.after.mir
#[custom_mir(dialect = "built")]
fn pointee(opt: &mut Option<i32>) {
    mir!(
        debug foo => Field::<i32>(Variant(*opt, 1), 0);
        {
            Return()
        }
    )
}

// EMIT_MIR debuginfo.numbered.built.after.mir
#[custom_mir(dialect = "analysis", phase = "post-cleanup")]
fn numbered(i: (u32, i32)) {
    mir!(
        debug first => i.0;
        debug second => i.0;
        {
            Return()
        }
    )
}

struct S { x: f32 }

// EMIT_MIR debuginfo.structured.built.after.mir
#[custom_mir(dialect = "analysis", phase = "post-cleanup")]
fn structured(i: S) {
    mir!(
        debug x => i.x;
        {
            Return()
        }
    )
}

// EMIT_MIR debuginfo.variant.built.after.mir
#[custom_mir(dialect = "built")]
fn variant(opt: Option<i32>) {
    mir!(
        debug inner => Field::<i32>(Variant(opt, 1), 0);
        {
            Return()
        }
    )
}

// EMIT_MIR debuginfo.variant_deref.built.after.mir
#[custom_mir(dialect = "built")]
fn variant_deref(opt: Option<&i32>) {
    mir!(
        debug pointer => Field::<&i32>(Variant(opt, 1), 0);
        debug deref => *Field::<&i32>(Variant(opt, 1), 0);
        {
            Return()
        }
    )
}

fn main() {
    numbered((5, 6));
    structured(S { x: 5. });
    variant(Some(5));
    variant_deref(Some(&5));
    pointee(&mut Some(5));
}
