use std::collections::HashMap;

use eyre::{eyre, Result};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::{
    visit::{self, Visit},
    ItemFn, Type,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub args: Vec<String>,
    pub ret: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Enum {
    pub repr: Repr,
    pub name: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Field {
    pub name: String,

    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Opaque {
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub exports: Vec<Function>,
    pub enums: Vec<Enum>,
    pub records: Vec<Record>,
    pub opaques: Vec<Opaque>,
}

impl Library {
    pub fn remap_types(&mut self) {
        // any opaque type will originally be NAME*, but platypus wants it to be NAME
        // so we need to remove the pointer symbol
        let mut depoint: HashMap<String, String> = HashMap::new();
        for Opaque { name } in &self.opaques {
            depoint.insert(format!("{}*", name), name.clone());
        }

        for function in &mut self.exports {
            for arg in &mut function.args {
                if let Some(replacement) = depoint.get(arg) {
                    arg.clone_from(replacement);
                }
            }
            if let Some(replacement) = depoint.get(&function.ret) {
                function.ret.clone_from(replacement);
            }
        }
    }
}

fn is_export(node: &ItemFn) -> bool {
    node.attrs.iter().any(|attribute| {
        let path = attribute.path();

        path.is_ident("export")
    })
}

#[derive(Debug, Default, Serialize, Deserialize, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "lowercase")]
pub enum Repr {
    #[strum(serialize = "C")]
    #[serde(rename = "enum")]
    #[default]
    C,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

fn get_repr(attributes: &[syn::Attribute]) -> Option<Repr> {
    let mut repr = None;
    for attribute in attributes {
        let path = attribute.path();
        if path.is_ident("repr") {
            let _ = attribute.parse_nested_meta(|meta| {
                let ident = meta.path.get_ident().map(|ident| ident.to_string());
                repr = ident.and_then(|ident| ident.parse().ok());
                Ok(())
            });
        }
    }

    repr
}

trait OpaqueItem {
    fn is_opaque(&self) -> bool;
}

impl From<&syn::ItemStruct> for Opaque {
    fn from(item: &syn::ItemStruct) -> Self {
        Self {
            name: item.ident.to_string(),
        }
    }
}

impl OpaqueItem for syn::ItemStruct {
    fn is_opaque(&self) -> bool {
        matches!(self.vis, syn::Visibility::Public(_))
            && self.attrs.iter().any(|attribute| {
                let path = attribute.path();

                path.is_ident("opaque")
            })
            && !is_record(self)
    }
}


impl From<&syn::ItemType> for Opaque {
    fn from(item: &syn::ItemType) -> Self {
        Self {
            name: item.ident.to_string(),
        }
    }
}

impl OpaqueItem for syn::ItemType {
    fn is_opaque(&self) -> bool {
        self.attrs.iter().any(|attribute| {
            let path = attribute.path();

            path.is_ident("opaque")
        })
    }
}


fn is_record(item_struct: &syn::ItemStruct) -> bool {
    matches!(item_struct.vis, syn::Visibility::Public(_))
        && item_struct.attrs.iter().any(|attribute| {
            let path = attribute.path();

            path.is_ident("record")
        })
}

fn fn_arg_type(arg: &syn::FnArg) -> Option<&Type> {
    match arg {
        syn::FnArg::Typed(pat) => Some(&pat.ty),
        _ => None,
    }
}

fn return_type(node: &ItemFn) -> Option<&Type> {
    match &node.sig.output {
        syn::ReturnType::Type(_, ty) => Some(ty),
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Library {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if node.is_opaque() {
            self.opaques.push(node.into());
        } else if is_record(node) {
            let name = node.ident.to_string();
            let fields = node
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap().to_string();
                    let ty = rust_to_perl_ffi_type(&field.ty).unwrap();
                    Field { name, ty }
                })
                .collect();
            self.records.push(Record { name, fields });
        }

        visit::visit_item_struct(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if node.is_opaque() {
            self.opaques.push(node.into());
        }

        visit::visit_item_type(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if let Some(repr) = get_repr(&node.attrs) {
            let name = node.ident.to_string();
            let variants = node
                .variants
                .iter()
                .map(|variant| {
                    let name = variant.ident.to_string();
                    let value = match &variant.discriminant {
                        Some((_, expr)) => expr.to_token_stream().to_string(),
                        None => name.clone(),
                    };
                    Variant { name, value }
                })
                .collect();
            self.enums.push(Enum {
                repr,
                name,
                variants,
            });
        }

        visit::visit_item_enum(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_export(node) {
            let name = node.sig.ident.to_string();
            let arg_types: Vec<String> = node
                .sig
                .inputs
                .iter()
                .flat_map(fn_arg_type)
                .map(rust_to_perl_ffi_type)
                .collect::<Result<Vec<String>>>()
                .unwrap();
            let ret_type = return_type(node)
                .map(rust_to_perl_ffi_type)
                .unwrap_or(Ok("void".to_string()))
                .unwrap();
            self.exports.push(Function {
                name,
                args: arg_types,
                ret: ret_type,
            });
        }

        // Delegate to the default impl to visit any nested functions.
        visit::visit_item_fn(self, node);
    }
}

// Function to convert Rust types to Platypus FFI types.
// Platypus supports most basic rust types, so those we can just pass through.
fn rust_to_perl_ffi_type(ty: &Type) -> Result<String> {
    match ty {
        Type::Array(ty) => rust_array_to_perl_ffi_type(ty),
        Type::BareFn(_) => Err(eyre!("function pointers are not supported")),
        Type::Group(_) => Err(eyre!("grouped types are not supported")),
        Type::ImplTrait(_) => Err(eyre!("impl trait is not supported")),
        Type::Infer(_) => Err(eyre!("inferred types are not supported")),
        Type::Macro(_) => Err(eyre!("macros are not supported")),
        Type::Never(_) => Err(eyre!("never type is not supported")),
        Type::Paren(_) => Err(eyre!("parenthesized types are not supported")),
        Type::Path(ty) => rust_path_to_perl_ffi_type(ty),
        Type::Ptr(ty) => rust_pointer_to_perl_ffi_type(ty),
        Type::Reference(_) => Err(eyre!("references are not supported")),
        Type::Slice(_) => Err(eyre!("slices are not supported")),
        Type::TraitObject(_) => Err(eyre!("trait objects are not supported")),
        Type::Tuple(_) => Err(eyre!("tuples are not supported")),
        Type::Verbatim(_) => Err(eyre!("verbatim types are not supported")),
        _ => Err(eyre!("unsupported type")),
    }
}

fn rust_array_to_perl_ffi_type(ty: &syn::TypeArray) -> Result<String> {
    let elem_ty = rust_to_perl_ffi_type(&ty.elem)?;
    let len = ty.len.to_token_stream().to_string();

    // special case for c_char to string
    if elem_ty == "c_char" {
        return Ok(format!("string({len})"));
    }

    Ok(format!("{elem_ty}[{len}]"))
}

fn rust_pointer_to_perl_ffi_type(ty: &syn::TypePtr) -> Result<String> {
    let elem_ty = rust_to_perl_ffi_type(&ty.elem)?;
    // special case for c_char to string
    if elem_ty == "c_char" {
        return Ok("string".to_string());
    }
    Ok(format!("{}*", elem_ty))
}

fn rust_path_to_perl_ffi_type(ty: &syn::TypePath) -> Result<String> {
    // special case array<T> to T[] in platypus
    if let Some(segment) = ty.path.segments.iter().next() {
        if segment.ident == "array" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let syn::GenericArgument::Type(ty) = args.args.iter().next().unwrap() {
                    return rust_to_perl_ffi_type(ty).map(|ty| format!("{}[]", ty));
                }
            }
        }
    }
    Ok(ty.path.to_token_stream().to_string())
}
