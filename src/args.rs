use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use minijinja::{value::Object, Value};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// This serves as the base package for the generated Perl module.
    #[clap(short, long)]
    name: String,

    /// The distribution name for the generated Perl module.
    #[clap(long)]
    distname: Option<String>,

    #[clap(long)]
    main_file: Option<PathBuf>,

    #[clap(short, long, default_value = "ffi/src/lib.rs")]
    pub input: PathBuf,

    #[clap(short, long, default_value = "ffi/Cargo.toml")]
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
        self.main_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("lib/{}.pm", self.name.replace("::", "/"))))
    }
}
