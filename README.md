plbindgen creates perl cpan distributions for Rust libraries that expose a public C API.
It uses `syn` perform a very rudimentary parse of the Rust source code to extract the
public C API. It then uses [FFI::Platypus](https://metacpan.org/pod/FFI::Platypus) and
[FFI::Platypus::Lang::Rust](https://metacpan.org/pod/FFI::Platypus::Lang::Rust) expose
that C API to Perl.

plbindgen is highly opinionated and is primarily designed to automate
the process of writing Platypus bindings to rust, in particular for [Rust::mysql](https://github.com/dylanwh/Rust-mysql).

If this description still doesn't make sense, this is basically a h2xs for rust,
that you will probably enjoy using a lot more than h2xs.

# SYNOPSIS

```bash
plbindgen -n Rust::mysql

cd Rust-mysql
perl Makefile.PL
make
make install
```


# USAGE

```bash
plbindgen --help

plbindgen - Generate Perl bindings for Rust code

This tool generates Perl bindings for Rust code using FFI::Platypus.

Usage: plbindgen [OPTIONS] --name <NAME>

Options:
  -n, --name <NAME>
          This serves as the base package for the generated Perl module

      --distname <DISTNAME>
          The name of the distribution, typically similar to the name but with dashes instead of colons

      --module-file <MODULE_FILE>
          The path to the main perl module file, relative to the root of the perl distribution

      --crate-file <CRATE_FILE>
          Path to the Rust crate file, relative to the root of the perl distribution
          
          [default: ffi/src/lib.rs]

      --cargo-toml <CARGO_TOML>
          Path to the Cargo.toml file for the Rust crate, relative to the root of the perl distribution
          
          [default: ffi/Cargo.toml]

  -h, --help
          Print help (see a summary with '-h')

```