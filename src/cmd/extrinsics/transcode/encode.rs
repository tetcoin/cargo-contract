// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of cargo-contract.
//
// cargo-contract is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// cargo-contract is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with cargo-contract.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::Result;
use itertools::Itertools;
use scale::{Compact, Encode, Output};
use scale_info::{form::{CompactForm, Form}, Field, RegistryReadOnly, TypeDef, TypeDefArray, TypeDefComposite, TypeDefVariant, TypeDefPrimitive, TypeDefSequence, Variant};
use std::{convert::TryInto, fmt::Debug, str::FromStr};
use super::{
    scon::Value,
    CompositeTypeFields,
};
use sp_core::sp_std::num::NonZeroU32;

pub trait EncodeValue {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()>;
}

pub fn encode_value<O>(registry: &RegistryReadOnly, type_id: NonZeroU32, value: &Value, output: &mut O) -> Result<()>
where
    O: Output + Debug
{
    let ty = registry.resolve(type_id)
        .ok_or(anyhow::anyhow!(
            "Failed to resolve type with id '{}'",
            type_id
        ))?;
    ty.type_def().encode_value_to(registry, value, output)
        .map_err(|e| anyhow::anyhow!("Error encoding value for {:?}: {}", ty.path(), e))
}

impl EncodeValue for TypeDef<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match self {
            TypeDef::Composite(composite) => composite.encode_value_to(registry, value, output),
            TypeDef::Variant(variant) => variant.encode_value_to(registry, value, output),
            TypeDef::Array(array) => array.encode_value_to(registry, value, output),
            TypeDef::Sequence(sequence) => sequence.encode_value_to(registry, value, output),
            TypeDef::Primitive(primitive) => primitive.encode_value_to(registry, value, output),
            def => unimplemented!("TypeDef::encode_value {:?}", def),
        }
    }
}

impl EncodeValue for TypeDefComposite<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let struct_type = CompositeTypeFields::from_type_def(&self)?;

        match value {
            Value::Map(map) => {
                // todo: should lookup via name so that order does not matter
                for (field, value) in self.fields().iter().zip(map.values()) {
                    field.encode_value_to(registry, value, output)?;
                }
                Ok(())
            },
            Value::Tuple(tuple) => {
                match struct_type {
                    CompositeTypeFields::TupleStructUnnamedFields(fields) => {
                        for (field, value) in fields.iter().zip(tuple.values()) {
                            field.encode_value_to(registry, value, output)?;
                        }
                        Ok(())
                    },
                    CompositeTypeFields::NoFields => {
                        Ok(())
                    },
                    CompositeTypeFields::StructNamedFields(_) => {
                        return Err(anyhow::anyhow!("Type is a struct requiring named fields"))
                    }
                }
            },
            v => {
                if let Ok(single_field) = self.fields().iter().exactly_one() {
                    single_field.encode_value_to(registry, value, output)
                } else {
                    Err(anyhow::anyhow!(
                        "Expected a Map or a Tuple or a single Value for a composite data type, found {:?}",
                        v
                    ))
                }
            }
        }
    }
}

impl EncodeValue for TypeDefVariant<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let variant_ident =
            match value {
                Value::Map(map) => {
                    map.ident().ok_or(anyhow::anyhow!("Missing enum variant identifier for map"))
                },
                Value::Tuple(tuple) => {
                    tuple.ident().ok_or(anyhow::anyhow!("Missing enum variant identifier for tuple"))
                },
                v => Err(anyhow::anyhow!("Invalid enum variant value '{:?}'", v))
            }?;

        let (index, variant) = self
            .variants()
            .iter()
            .find_position(|v| v.name() == &variant_ident)
            .ok_or(anyhow::anyhow!("No variant '{}' found", variant_ident))?;

        let index: u8 = index.try_into().map_err(|_| anyhow::anyhow!("Variant index > 255"))?;
        output.push_byte(index);

        variant.encode_value_to(registry, value, output)
    }
}

impl EncodeValue for Variant<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match value {
            Value::Map(_map) => {
                // todo: should lookup via name so that order does not matter
                // for (field, value) in self.fields().iter().zip(map.values()) {
                //     field.encode_value_to(registry, value, output)?;
                // }
                // Ok(())
                todo!()
            },
            Value::Tuple(tuple) => {
                for (field, value) in self.fields().iter().zip(tuple.values()) {
                    field.encode_value_to(registry, value, output)?;
                }
                Ok(())
            },
            v => Err(anyhow::anyhow!("Invalid enum variant value '{:?}'", v))
        }
    }
}

impl EncodeValue for Field<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encode_value(registry, self.ty().id(), value, output)
    }
}

impl EncodeValue for TypeDefArray<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encode_seq(self.type_param(), registry, value, false, output)
    }
}

impl EncodeValue for TypeDefSequence<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        registry: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encode_seq(self.type_param(), registry, value, true, output)
    }
}

fn encode_seq<O: Output + Debug>(
    ty: &<CompactForm as Form>::Type,
    registry: &RegistryReadOnly,
    value: &Value,
    encode_len: bool,
    output: &mut O,
) -> Result<()> {
    let ty = registry.resolve(ty.id())
        .ok_or(anyhow::anyhow!("Failed to find type with id '{}'", ty.id()))?;
    match value {
        Value::Seq(values) => {
            if encode_len {
                Compact(values.len() as u32).encode_to(output);
            }
            for value in values.elems() {
                ty.type_def().encode_value_to(registry, value, output)?;
            }
        },
        Value::Bytes(bytes) => {
            if encode_len {
                Compact(bytes.bytes().len() as u32).encode_to(output);
            }
            for byte in bytes.bytes() {
                output.push_byte(*byte);
            }
        }
        value => return Err(anyhow::anyhow!("{:?} cannot be encoded as an array", value)),
    }
    Ok(())
}

impl EncodeValue for TypeDefPrimitive {
    fn encode_value_to<O: Output + Debug>(
        &self,
        _: &RegistryReadOnly,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match self {
            TypeDefPrimitive::Bool => {
                if let Value::Bool(b) = value {
                    b.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a bool value"))
                }
            }
            TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
            TypeDefPrimitive::Str => {
                if let Value::String(s) = value {
                    s.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a String value"))
                }
            }
            TypeDefPrimitive::U8 => {
                if let Value::UInt(i) = value {
                    let u: u8 = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a u8 value"))
                }
            }
            TypeDefPrimitive::U16 => {
                if let Value::UInt(i) = value {
                    let u: u16 = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a u16 value"))
                }
            }
            TypeDefPrimitive::U32 => {
                if let Value::UInt(i) = value {
                    let u: u32 = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a u16 value"))
                }
            }
            TypeDefPrimitive::U64 => match value {
                Value::UInt(i) => {
                    let u: u64 = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                }
                Value::String(s) => {
                    let sanitized = s.replace(&['_', ','][..], "");
                    let u: u64 = u64::from_str(&sanitized)?;
                    u.encode_to(output);
                    Ok(())
                }
                _ => Err(anyhow::anyhow!("Expected a Number or a String value")),
            },
            TypeDefPrimitive::U128 => match value {
                Value::UInt(i) => {
                    let u: u128 = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                }
                Value::String(s) => {
                    let sanitized = s.replace(&['_', ','][..], "");
                    let u: u128 = u128::from_str(&sanitized)?;
                    u.encode_to(output);
                    Ok(())
                }
                _ => Err(anyhow::anyhow!("Expected a Number or a String value")),
            },

            _ => unimplemented!("TypeDefPrimitive::encode_value"),
            // TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
            // TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
            // TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
            // TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
            // TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
        }
    }
}
