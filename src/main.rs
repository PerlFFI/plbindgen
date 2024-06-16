mod api;
mod args;
mod templates;

use api::{Library, Struct};
use args::Args;
use eyre::{eyre, Context, Result};
use minijinja::{context, Value};
use std::{
    collections::HashMap,
    fs::{self, create_dir_all},
};
use syn::visit::Visit;

fn main() -> Result<()> {
    let args = Args::new();

    let code = fs::read_to_string(&args.input)
        .wrap_err(eyre!("failed to read {}", args.input.display()))?;
    let file =
        syn::parse_file(&code).wrap_err(eyre!("failed to parse {}", args.input.display()))?;

    let mut lib = Library::default();
    lib.visit_file(&file);
    lib.remap_types();
    let lib = lib;

    let cargo = fs::read_to_string(&args.cargo_toml)
        .wrap_err(eyre!("failed to read {}", args.cargo_toml.display()))?;
    let cargo: toml::Value = toml::from_str(&cargo)?;

    let mut env = templates::new()?;
    env.add_global("args", Value::from_object(args.clone()));
    env.add_global("lib", Value::from_serialize(&lib));
    env.add_global("cargo", Value::from_serialize(&cargo));

    let package = env.get_template("package.j2")?;
    let s = package.render(context! {})?;

    if let Some(parent) = args.main_file().parent() {
        create_dir_all(parent)?;
    }
    fs::write(args.main_file(), s)?;

    let makefile = env.get_template("makefile.pl.j2")?;
    let s = makefile.render(context! {})?;
    fs::write("Makefile.PL", s)?;

    for record in lib.structs {
        let template = env.get_template("record.j2")?;
        let s = template.render(context! {
            record => record,
        })?;
        // if main_file is lib/Foo/Bar.pm, we want lib/Foo/Bar/
        let file = args.main_file();
        let Some(base) = file.file_stem() else {
            return Err(eyre!("main_file must be a file"));
        };
        let Some(dir) = file.parent().map(|p| p.join(base)) else {
            return Err(eyre!("main_file must have a parent"));
        };
        create_dir_all(&dir)?;
        fs::write(dir.join(format!("{}.pm", record.name)), s)?;
    }

    Ok(())
}
