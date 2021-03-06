// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::FuzzTargetImpl;
use anyhow::{bail, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use libra_proptest_helpers::ValueGenerator;
use move_vm_types::{
    loaded_data::{struct_def::StructDef, types::Type},
    values::{prop::layout_and_value_strategy, Value},
};
use std::io::Cursor;

#[derive(Clone, Debug, Default)]
pub struct ValueTarget;

impl FuzzTargetImpl for ValueTarget {
    fn name(&self) -> &'static str {
        module_name!()
    }

    fn description(&self) -> &'static str {
        "VM values + types (custom deserializer)"
    }

    fn generate(&self, _idx: usize, gen: &mut ValueGenerator) -> Option<Vec<u8>> {
        let (layout, value) = gen.generate(layout_and_value_strategy());

        // Values as currently serialized are not self-describing, so store a serialized form of the
        // type along with the value as well.
        let layout_blob = lcs::to_bytes(&layout).unwrap();
        let value_blob = value.simple_serialize(&layout).expect("must serialize");

        let mut blob = vec![];
        // Prefix the layout blob with its length.
        blob.write_u64::<BigEndian>(layout_blob.len() as u64)
            .expect("writing should work");
        blob.extend_from_slice(&layout_blob);
        blob.extend_from_slice(&value_blob);
        Some(blob)
    }

    fn fuzz(&self, data: &[u8]) {
        let _ = deserialize(data);
    }
}

fn is_valid_layout(layout: &Type) -> bool {
    use Type::*;

    match layout {
        Bool | U8 | U64 | U128 | Address => true,

        Reference(_) | MutableReference(_) | TypeVariable(_) => false,

        Vector(layout) => is_valid_layout(layout),

        Struct(StructDef::Struct(inner)) => {
            let field_layouts = inner.field_definitions();
            if field_layouts.is_empty() {
                return false;
            }
            field_layouts.iter().all(is_valid_layout)
        }
        Struct(StructDef::Native(_)) => false,
    }
}

fn deserialize(data: &[u8]) -> Result<()> {
    let mut data = Cursor::new(data);
    // Read the length of the layout blob.
    let len = data.read_u64::<BigEndian>()? as usize;
    let position = data.position() as usize;
    let data = &data.into_inner()[position..];

    if data.len() < len {
        bail!("too little data");
    }
    let layout_data = &data[0..len];
    let value_data = &data[len..];

    let layout: Type = lcs::from_bytes(layout_data)?;
    // The fuzzer may alter the raw bytes, resulting in invalid layouts that will not
    // pass the bytecode verifier. We need to filter these out as they can show up as
    // false positives.
    if !is_valid_layout(&layout) {
        bail!("invalid layout");
    }
    let _ = Value::simple_deserialize(value_data, layout);
    Ok(())
}
