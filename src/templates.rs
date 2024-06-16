use std::borrow::Cow;

use eyre::{eyre, Context, ContextCompat};
use minijinja::Environment;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "templates"]
struct Templates;

pub fn new<'a>() -> eyre::Result<Environment<'a>> {
    let mut env = Environment::new();
    for path in Templates::iter() {
        let template = Templates::get(&path).wrap_err(eyre!("Failed to read template {path}"))?;
        let source: String = std::str::from_utf8(&template.data)
            .wrap_err(eyre!("Failed to parse template {path}"))?
            .into();
        env.add_template_owned(path, source)?;
    }
    env.add_filter("perl_quote", |value: Cow<str>| perl_quote(value));

    Ok(env)
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
