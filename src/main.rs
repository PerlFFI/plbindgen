#![allow(unused_imports)]
#![allow(unused)]

use eyre::eyre;
use eyre::Result;
use quote::quote;
use quote::ToTokens;
use std::collections::HashMap;
use std::fs;
use syn::token::Const;
use syn::token::Extern;
use syn::visit::{self, Visit};
use syn::Abi;
use syn::LitStr;
use syn::{File, Item, ItemFn, ItemForeignMod, Type};

#[derive(Debug, Default)]
struct Function {
    name: String,
    arg_types: Vec<String>,
    ret_type: String,
}

#[derive(Debug, Default)]
struct Enum {
    name: String,
    variants: Vec<(String, u32)>,
}

#[derive(Debug, Default)]
struct Struct {
    name: String,
    fields: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct ForeignVisitor {
    functions: Vec<Function>,
    enums: Vec<Enum>,
    structs: Vec<Struct>,
    opaque_structs: Vec<String>,
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

impl<'ast> Visit<'ast> for ForeignVisitor {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if is_opaque_struct(node) {
            let name = node.ident.to_string();
            self.opaque_structs.push(name);
        } else if is_repr_c(&node.attrs) {
            let name = node.ident.to_string();
            let fields = node
                .fields
                .iter()
                .map(|field| {
                    let name = field.ident.as_ref().unwrap().to_string();
                    let ty = rust_to_perl_ffi_type(&field.ty).unwrap();
                    (name, ty)
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
                    let discriminant = variant
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
                    (name, discriminant)
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
                arg_types,
                ret_type,
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

fn main() {
    let code = fs::read_to_string("src/lib.rs").expect("Unable to read file");
    let file: File = syn::parse_file(&code).expect("Unable to parse file");

    let mut visitor = ForeignVisitor::default();
    visitor.visit_file(&file);
    let mut typemap: HashMap<String, String> = HashMap::new();

    for Enum { name, variants } in &visitor.enums {
        for (variant, value) in variants {
            println!("use constant {variant} => {value};");
        }
        println!("$ffi->type('enum', {name});", name = perl_quote(name));
        typemap.insert(name.clone(), "enum".to_string());
    }

    for name in &visitor.opaque_structs {
        println!("$ffi->type('opaque', {name});", name = perl_quote(name));
        typemap.insert(format!("{name}*"), name.clone());
    }

    for Function {
        name,
        arg_types,
        ret_type,
    } in &visitor.functions
    {
        println!(
            "$ffi->attach({name} => [{arg_types}] => {ret_type})",
            arg_types = arg_types
                .iter()
                .map(|name| typemap.get(name).unwrap_or(name))
                .map(perl_quote)
                .collect::<Vec<_>>()
                .join(", "),
            ret_type = perl_quote(typemap.get(ret_type).unwrap_or(ret_type))
        );
    }
}

fn perl_quote<S>(s: S) -> String
where
    S: AsRef<str>,
{
    let mut quoted = String::new();
    for c in s.as_ref().chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '\'' => quoted.push_str("\\'"),
            _ => quoted.push(c),
        }
    }
    format!("'{}'", quoted)
}
