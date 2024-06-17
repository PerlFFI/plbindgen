use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use minijinja::{value::Object, Value};

/// plbindgen - Generate Perl bindings for Rust code
/// 
/// This tool generates Perl bindings for Rust code using FFI::Platypus.
#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// This serves as the base package for the generated Perl module.
    #[clap(short, long)]
    name: String,

    /// The name of the distribution, typically similar to the name but with dashes instead of colons.
    #[clap(long)]
    distname: Option<String>,

    /// The path to the main perl module file, relative to the root of the perl distribution.
    #[clap(long)]
    module_file: Option<PathBuf>,
    
    /// Path to the Rust crate file, relative to the root of the perl distribution.
    #[clap(long, default_value = "ffi/src/lib.rs")]
    pub crate_file: PathBuf,

    /// Path to the Cargo.toml file for the Rust crate, relative to the root of the
    /// perl distribution.
    #[clap(long, default_value = "ffi/Cargo.toml")]
    pub cargo_toml: PathBuf,
}

impl Object for Args {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "name" => Some(Value::from(self.name())),
            "distname" => Some(Value::from(self.distname())),
            "main_file" => {
                let file = self.main_file();
                Some(Value::from(file.to_string_lossy().to_string()))
            }
            _ => None,
        }
    }
}

impl Args {
    pub fn new() -> Self {
        Args::parse()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn distname(&self) -> String {
        self.distname
            .clone()
            .unwrap_or_else(|| self.name.replace("::", "-"))
    }

    pub fn main_file(&self) -> PathBuf {
        self.module_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("lib/{}.pm", self.name.replace("::", "/"))))
    }
}
