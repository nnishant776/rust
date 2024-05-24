use super::{Byte, Def, Ref};
use std::ops::ControlFlow;

#[cfg(test)]
mod tests;

/// A tree-based representation of a type layout.
///
/// Invariants:
/// 1. All paths through the layout have the same length (in bytes).
///
/// Nice-to-haves:
/// 1. An `Alt` is never directly nested beneath another `Alt`.
/// 2. A `Seq` is never directly nested beneath another `Seq`.
/// 3. `Seq`s and `Alt`s with a single member do not exist.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum Tree<D, R>
where
    D: Def,
    R: Ref,
{
    /// A sequence of successive layouts.
    Seq(Vec<Self>),
    /// A choice between alternative layouts.
    Alt(Vec<Self>),
    /// A definition node.
    Def(D),
    /// A reference node.
    Ref(R),
    /// A byte node.
    Byte(Byte),
}

impl<D, R> Tree<D, R>
where
    D: Def,
    R: Ref,
{
    /// A `Tree` consisting only of a definition node.
    pub(crate) fn def(def: D) -> Self {
        Self::Def(def)
    }

    /// A `Tree` representing an uninhabited type.
    pub(crate) fn uninhabited() -> Self {
        Self::Alt(vec![])
    }

    /// A `Tree` representing a zero-sized type.
    pub(crate) fn unit() -> Self {
        Self::Seq(Vec::new())
    }

    /// A `Tree` containing a single, uninitialized byte.
    pub(crate) fn uninit() -> Self {
        Self::Byte(Byte::Uninit)
    }

    /// A `Tree` representing the layout of `bool`.
    pub(crate) fn bool() -> Self {
        Self::from_bits(0x00).or(Self::from_bits(0x01))
    }

    /// A `Tree` whose layout matches that of a `u8`.
    pub(crate) fn u8() -> Self {
        Self::Alt((0u8..=255).map(Self::from_bits).collect())
    }

    /// A `Tree` whose layout accepts exactly the given bit pattern.
    pub(crate) fn from_bits(bits: u8) -> Self {
        Self::Byte(Byte::Init(bits))
    }

    /// A `Tree` whose layout is a number of the given width.
    pub(crate) fn number(width_in_bytes: usize) -> Self {
        Self::Seq(vec![Self::u8(); width_in_bytes])
    }

    /// A `Tree` whose layout is entirely padding of the given width.
    pub(crate) fn padding(width_in_bytes: usize) -> Self {
        Self::Seq(vec![Self::uninit(); width_in_bytes])
    }

    /// Remove all `Def` nodes, and all branches of the layout for which `f`
    /// produces `true`.
    pub(crate) fn prune<F>(self, f: &F) -> Tree<!, R>
    where
        F: Fn(D) -> bool,
    {
        match self {
            Self::Seq(elts) => match elts.into_iter().map(|elt| elt.prune(f)).try_fold(
                Tree::unit(),
                |elts, elt| {
                    if elt == Tree::uninhabited() {
                        ControlFlow::Break(Tree::uninhabited())
                    } else {
                        ControlFlow::Continue(elts.then(elt))
                    }
                },
            ) {
                ControlFlow::Break(node) | ControlFlow::Continue(node) => node,
            },
            Self::Alt(alts) => alts
                .into_iter()
                .map(|alt| alt.prune(f))
                .fold(Tree::uninhabited(), |alts, alt| alts.or(alt)),
            Self::Byte(b) => Tree::Byte(b),
            Self::Ref(r) => Tree::Ref(r),
            Self::Def(d) => {
                if f(d) {
                    Tree::uninhabited()
                } else {
                    Tree::unit()
                }
            }
        }
    }

    /// Produces `true` if `Tree` is an inhabited type; otherwise false.
    pub(crate) fn is_inhabited(&self) -> bool {
        match self {
            Self::Seq(elts) => elts.into_iter().all(|elt| elt.is_inhabited()),
            Self::Alt(alts) => alts.into_iter().any(|alt| alt.is_inhabited()),
            Self::Byte(..) | Self::Ref(..) | Self::Def(..) => true,
        }
    }
}

impl<D, R> Tree<D, R>
where
    D: Def,
    R: Ref,
{
    /// Produces a new `Tree` where `other` is sequenced after `self`.
    pub(crate) fn then(self, other: Self) -> Self {
        match (self, other) {
            (Self::Seq(elts), other) | (other, Self::Seq(elts)) if elts.len() == 0 => other,
            (Self::Seq(mut lhs), Self::Seq(mut rhs)) => {
                lhs.append(&mut rhs);
                Self::Seq(lhs)
            }
            (Self::Seq(mut lhs), rhs) => {
                lhs.push(rhs);
                Self::Seq(lhs)
            }
            (lhs, Self::Seq(mut rhs)) => {
                rhs.insert(0, lhs);
                Self::Seq(rhs)
            }
            (lhs, rhs) => Self::Seq(vec![lhs, rhs]),
        }
    }

    /// Produces a new `Tree` accepting either `self` or `other` as alternative layouts.
    pub(crate) fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Alt(alts), other) | (other, Self::Alt(alts)) if alts.len() == 0 => other,
            (Self::Alt(mut lhs), Self::Alt(rhs)) => {
                lhs.extend(rhs);
                Self::Alt(lhs)
            }
            (Self::Alt(mut alts), alt) | (alt, Self::Alt(mut alts)) => {
                alts.push(alt);
                Self::Alt(alts)
            }
            (lhs, rhs) => Self::Alt(vec![lhs, rhs]),
        }
    }
}

#[cfg(feature = "rustc")]
pub(crate) mod rustc {
    use super::Tree;
    use crate::layout::rustc::{Def, Ref};

    use rustc_middle::ty::layout::HasTyCtxt;
    use rustc_middle::ty::layout::LayoutCx;
    use rustc_middle::ty::layout::LayoutError;
    use rustc_middle::ty::layout::LayoutOf;
    use rustc_middle::ty::AdtDef;
    use rustc_middle::ty::AdtKind;
    use rustc_middle::ty::List;
    use rustc_middle::ty::ScalarInt;
    use rustc_middle::ty::{self, Ty, TyCtxt, TypeVisitableExt};
    use rustc_span::ErrorGuaranteed;
    use rustc_target::abi::FieldsShape;
    use rustc_target::abi::Size;
    use rustc_target::abi::TyAndLayout;
    use rustc_target::abi::Variants;

    #[derive(Debug, Copy, Clone)]
    pub(crate) enum Err {
        /// The layout of the type is not yet supported.
        NotYetSupported,
        /// This error will be surfaced elsewhere by rustc, so don't surface it.
        UnknownLayout,
        /// Overflow size
        SizeOverflow,
        TypeError(ErrorGuaranteed),
    }

    impl<'tcx> From<&LayoutError<'tcx>> for Err {
        fn from(err: &LayoutError<'tcx>) -> Self {
            match err {
                LayoutError::Unknown(..) | LayoutError::ReferencesError(..) => Self::UnknownLayout,
                LayoutError::SizeOverflow(..) => Self::SizeOverflow,
                LayoutError::Cycle(err) => Self::TypeError(*err),
                err => unimplemented!("{:?}", err),
            }
        }
    }

    impl<'tcx> Tree<Def<'tcx>, Ref<'tcx>> {
        pub fn from_ty(
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            use rustc_target::abi::HasDataLayout;

            if let Err(e) = ty_and_layout.ty.error_reported() {
                return Err(Err::TypeError(e));
            }

            let target = cx.tcx.data_layout();
            let pointer_size = target.pointer_size;

            match ty_and_layout.ty.kind() {
                ty::Bool => Ok(Self::bool()),

                ty::Float(nty) => {
                    let width = nty.bit_width() / 8;
                    Ok(Self::number(width as _))
                }

                ty::Int(nty) => {
                    let width = nty.normalize(pointer_size.bits() as _).bit_width().unwrap() / 8;
                    Ok(Self::number(width as _))
                }

                ty::Uint(nty) => {
                    let width = nty.normalize(pointer_size.bits() as _).bit_width().unwrap() / 8;
                    Ok(Self::number(width as _))
                }

                ty::Tuple(members) => Self::from_tuple(ty_and_layout, members, cx),

                ty::Array(inner_ty, len) => {
                    let FieldsShape::Array { stride, count } = &ty_and_layout.fields else {
                        return Err(Err::NotYetSupported);
                    };
                    let inner_ty_and_layout = cx.layout_of(*inner_ty)?;
                    assert_eq!(*stride, inner_ty_and_layout.size);
                    let elt = Tree::from_ty(inner_ty_and_layout, cx)?;
                    Ok(std::iter::repeat(elt)
                        .take(*count as usize)
                        .fold(Tree::unit(), |tree, elt| tree.then(elt)))
                }

                ty::Adt(adt_def, _args_ref) if !ty_and_layout.ty.is_box() => {
                    match adt_def.adt_kind() {
                        AdtKind::Struct => Self::from_struct(ty_and_layout, *adt_def, cx),
                        AdtKind::Enum => Self::from_enum(ty_and_layout, *adt_def, cx),
                        AdtKind::Union => Self::from_union(ty_and_layout, *adt_def, cx),
                    }
                }

                ty::Ref(lifetime, ty, mutability) => {
                    let ty_and_layout = cx.layout_of(*ty)?;
                    let align = ty_and_layout.align.abi.bytes_usize();
                    let size = ty_and_layout.size.bytes_usize();
                    Ok(Tree::Ref(Ref {
                        lifetime: *lifetime,
                        ty: *ty,
                        mutability: *mutability,
                        align,
                        size,
                    }))
                }

                _ => Err(Err::NotYetSupported),
            }
        }

        /// Constructs a `Tree` from a tuple.
        fn from_tuple(
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            members: &'tcx List<Ty<'tcx>>,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            match &ty_and_layout.fields {
                FieldsShape::Primitive => {
                    assert_eq!(members.len(), 1);
                    let inner_ty = members[0];
                    let inner_ty_and_layout = cx.layout_of(inner_ty)?;
                    assert_eq!(ty_and_layout.layout, inner_ty_and_layout.layout);
                    Self::from_ty(inner_ty_and_layout, cx)
                }
                FieldsShape::Arbitrary { offsets, .. } => {
                    assert_eq!(offsets.len(), members.len());
                    Self::from_variant(Def::Primitive, None, ty_and_layout, ty_and_layout.size, cx)
                }
                FieldsShape::Array { .. } | FieldsShape::Union(_) => Err(Err::NotYetSupported),
            }
        }

        /// Constructs a `Tree` from a struct.
        ///
        /// # Panics
        ///
        /// Panics if `def` is not a struct definition.
        fn from_struct(
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            def: AdtDef<'tcx>,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            assert!(def.is_struct());
            let def = Def::Adt(def);
            Self::from_variant(def, None, ty_and_layout, ty_and_layout.size, cx)
        }

        /// Constructs a `Tree` from an enum.
        ///
        /// # Panics
        ///
        /// Panics if `def` is not an enum definition.
        fn from_enum(
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            def: AdtDef<'tcx>,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            assert!(def.is_enum());
            let layout = ty_and_layout.layout;

            if let Variants::Multiple { tag_field, .. } = layout.variants() {
                // For enums (but not coroutines), the tag field is
                // currently always the first field of the layout.
                assert_eq!(*tag_field, 0);
            }

            let variants = def.discriminants(cx.tcx()).try_fold(
                Self::uninhabited(),
                |variants, (idx, ref discriminant)| {
                    let tag = cx.tcx.tag_for_variant((ty_and_layout.ty, idx));
                    let variant_def = Def::Variant(def.variant(idx));
                    let variant_ty_and_layout = ty_and_layout.for_variant(&cx, idx);
                    let variant = Self::from_variant(
                        variant_def,
                        tag,
                        variant_ty_and_layout,
                        layout.size,
                        cx,
                    )?;
                    Result::<Self, Err>::Ok(variants.or(variant))
                },
            )?;

            return Ok(Self::def(Def::Adt(def)).then(variants));
        }

        /// Constructs a `Tree` from a 'variant-like' layout.
        ///
        /// A 'variant-like' layout includes those of structs and, of course,
        /// enum variants. Pragmatically speaking, this method supports anything
        /// with `FieldsShape::Arbitrary`.
        ///
        /// Note: This routine assumes that the optional `tag` is the first
        /// field, and enum callers should check that `tag_field` is, in fact,
        /// `0`.
        fn from_variant(
            def: Def<'tcx>,
            tag: Option<ScalarInt>,
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            total_size: Size,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            // This constructor does not support non-`FieldsShape::Arbitrary`
            // layouts.
            let FieldsShape::Arbitrary { offsets, memory_index } = ty_and_layout.layout.fields()
            else {
                return Err(Err::NotYetSupported);
            };

            // When this function is invoked with enum variants,
            // `ty_and_layout.size` does not encompass the entire size of the
            // enum. We rely on `total_size` for this.
            assert!(ty_and_layout.size <= total_size);

            let mut size = Size::ZERO;
            let mut struct_tree = Self::def(def);

            // If a `tag` is provided, place it at the start of the layout.
            if let Some(tag) = tag {
                size += tag.size();
                struct_tree = struct_tree.then(Self::from_tag(tag, cx.tcx));
            }

            // Append the fields, in memory order, to the layout.
            let inverse_memory_index = memory_index.invert_bijective_mapping();
            for (memory_idx, field_idx) in inverse_memory_index.iter_enumerated() {
                // Add interfield padding.
                let padding_needed = offsets[*field_idx] - size;
                let padding = Self::padding(padding_needed.bytes_usize());

                let field_ty_and_layout = ty_and_layout.field(&cx, field_idx.as_usize());
                let field_tree = Self::from_ty(field_ty_and_layout, cx)?;

                struct_tree = struct_tree.then(padding).then(field_tree);

                size += padding_needed + field_ty_and_layout.size;
            }

            // Add trailing padding.
            let padding_needed = total_size - size;
            let trailing_padding = Self::padding(padding_needed.bytes_usize());

            Ok(struct_tree.then(trailing_padding))
        }

        /// Constructs a `Tree` representing the value of a enum tag.
        fn from_tag(tag: ScalarInt, tcx: TyCtxt<'tcx>) -> Self {
            use rustc_target::abi::Endian;
            let size = tag.size();
            let bits = tag.assert_bits(size);
            let bytes: [u8; 16];
            let bytes = match tcx.data_layout.endian {
                Endian::Little => {
                    bytes = bits.to_le_bytes();
                    &bytes[..size.bytes_usize()]
                }
                Endian::Big => {
                    bytes = bits.to_be_bytes();
                    &bytes[bytes.len() - size.bytes_usize()..]
                }
            };
            Self::Seq(bytes.iter().map(|&b| Self::from_bits(b)).collect())
        }

        /// Constructs a `Tree` from a union.
        ///
        /// # Panics
        ///
        /// Panics if `def` is not a union definition.
        fn from_union(
            ty_and_layout: TyAndLayout<'tcx, Ty<'tcx>>,
            def: AdtDef<'tcx>,
            cx: LayoutCx<'tcx, TyCtxt<'tcx>>,
        ) -> Result<Self, Err> {
            assert!(def.is_union());

            let union_layout = ty_and_layout.layout;

            // This constructor does not support non-`FieldsShape::Union`
            // layouts. Fields of this shape are all placed at offset 0.
            let FieldsShape::Union(fields) = union_layout.fields() else {
                return Err(Err::NotYetSupported);
            };

            let fields = &def.non_enum_variant().fields;
            let fields = fields.iter_enumerated().try_fold(
                Self::uninhabited(),
                |fields, (idx, ref field_def)| {
                    let field_def = Def::Field(field_def);
                    let field_ty_and_layout = ty_and_layout.field(&cx, idx.as_usize());
                    let field = Self::from_ty(field_ty_and_layout, cx)?;
                    let trailing_padding_needed = union_layout.size - field_ty_and_layout.size;
                    let trailing_padding = Self::padding(trailing_padding_needed.bytes_usize());
                    let field_and_padding = field.then(trailing_padding);
                    Result::<Self, Err>::Ok(fields.or(field_and_padding))
                },
            )?;

            Ok(Self::def(Def::Adt(def)).then(fields))
        }
    }
}
