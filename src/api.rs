use std::collections::{HashMap, HashSet};

use eyre::{eyre, Result};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::{
    visit::{self, Visit},
    Abi, ItemFn, Type,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub args: Vec<String>,
    pub ret: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    pub value: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Struct {
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
pub struct Library {
    pub functions: Vec<Function>,
    pub enums: Vec<Enum>,
    pub structs: Vec<Struct>,
    pub opaques: Vec<String>,
}

impl Library {
    pub fn remap_types(&mut self) {
        // any opaque type will originally be NAME*, but platypus wants it to be NAME
        // so we need to remove the pointer symbol
        let mut depoint: HashMap<String, String> = HashMap::new();
        for opaque in &self.opaques {
            depoint.insert(format!("{}*", opaque), opaque.clone());
        }

        for function in &mut self.functions {
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

fn is_extern_c(node: &ItemFn) -> bool {
    matches!(&node.sig.abi, Some(Abi {
            name: Some(refname),
            ..
        }) if refname.value().as_str() == "C")
}

fn is_public(node: &ItemFn) -> bool {
    matches!(&node.vis, syn::Visibility::Public(_))
}

fn is_no_mangle(node: &ItemFn) -> bool {
    node.attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("no_mangle"))
}

fn is_exported(node: &ItemFn) -> bool {
    is_extern_c(node) && is_public(node) && is_no_mangle(node)
}

fn is_repr_c(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        let path = attribute.path();
        let mut is_repr_c = false;
        if path.is_ident("repr") {
            attribute
                .parse_nested_meta(|meta| {
                    if meta.path.is_ident("C") {
                        is_repr_c = true;
                    }

                    Ok(())
                })
                .expect("Failed to parse repr attribute");
        }
        is_repr_c
    })
}

fn is_simple_enum(node: &syn::ItemEnum) -> bool {
    is_repr_c(&node.attrs)
        && node
            .variants
            .iter()
            .all(|variant| variant.fields.is_empty())
}

fn is_public_struct(node: &syn::ItemStruct) -> bool {
    matches!(&node.vis, syn::Visibility::Public(_))
}

fn is_opaque_struct(node: &syn::ItemStruct) -> bool {
    !is_repr_c(&node.attrs) && is_public_struct(node)
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
        if is_opaque_struct(node) {
            let name = node.ident.to_string();
            self.opaques.push(name);
        } else if is_repr_c(&node.attrs) {
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
            self.structs.push(Struct { name, fields });
        }

        visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if is_simple_enum(node) {
            let name = node.ident.to_string();
            let variants = node
                .variants
                .iter()
                .map(|variant| {
                    let name = variant.ident.to_string();
                    let value = variant
                        .discriminant
                        .as_ref()
                        .and_then(|(_, expr)| {
                            if let syn::Expr::Lit(lit) = expr {
                                if let syn::Lit::Int(int) = &lit.lit {
                                    Some(int.base10_parse::<u32>().unwrap())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    Variant { name, value }
                })
                .collect();
            self.enums.push(Enum { name, variants });
        }

        visit::visit_item_enum(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_exported(node) {
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
            self.functions.push(Function {
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
        return Ok(format!("string({len})"))
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
    Ok(ty.path.to_token_stream().to_string())
}
