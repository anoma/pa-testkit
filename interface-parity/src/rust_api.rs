use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow};
use public_api::tokens::Token;

use crate::cmd;
use crate::compare::Surface;
use crate::packages::has_lib;

/// The toolchain whose rustdoc JSON `public-api` 0.52.2 reads. The justfile's
/// `nightly` variable names the same toolchain for `just install-nightly`.
pub(crate) const NIGHTLY: &str = "nightly-2026-02-08";

/// rustup installs a toolchain it is asked for unless told not to, and
/// concurrent installs of one toolchain fail; a missing toolchain is an error.
const NO_AUTO_INSTALL: (&str, &str) = ("RUSTUP_AUTO_INSTALL", "0");

/// The pinned toolchain's binary `name`.
fn nightly_binary(name: &str) -> anyhow::Result<PathBuf> {
    let path = cmd::stdout(
        Command::new("rustup")
            .env(NO_AUTO_INSTALL.0, NO_AUTO_INSTALL.1)
            .args(["which", "--toolchain", NIGHTLY, name]),
    )
    .context("install the toolchain with `just install-nightly`")?;
    Ok(PathBuf::from(path.trim()))
}

/// A crate's public API: its items keyed for comparison, and which file
/// declares each of them.
#[derive(Debug, Default)]
pub struct RustApi {
    pub surface: Surface,
    /// Each source file, as rustdoc names it (relative to the workspace
    /// root), with the (key, rendering) of every item it declares. An impl
    /// the compiler supplies, like `Send`, is declared in no file.
    pub declared: BTreeMap<PathBuf, Vec<(String, String)>>,
}

/// The public API of the package's library target, built with all features
/// and the repository's build environment `env`. A package without a library
/// target exposes no Rust items.
pub fn api(meta: &cargo_metadata::Package, env: &[(String, String)]) -> anyhow::Result<RustApi> {
    if !has_lib(meta) {
        return Ok(RustApi::default());
    }
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    // rustdoc JSON only loads dependencies compiled by the same rustc, so both
    // binaries come from the pinned toolchain whatever PATH resolves first.
    let builder = env
        .iter()
        .fold(rustdoc_json::Builder::default(), |builder, (key, value)| {
            builder.env(key, value)
        });
    let json = builder
        .toolchain(NIGHTLY)
        .env(NO_AUTO_INSTALL.0, NO_AUTO_INSTALL.1)
        .env("RUSTC", nightly_binary("rustc")?)
        .env("RUSTDOC", nightly_binary("rustdoc")?)
        .manifest_path(&meta.manifest_path)
        .package(meta.name.as_str())
        .all_features(true)
        .build_with_captured_output(&mut stdout, &mut stderr)
        .map_err(|e| anyhow!("{e}\n{}", String::from_utf8_lossy(&stderr)))?;
    let (renamed, files) = rename_crate(&json)?;
    let public = public_api::Builder::from_rustdoc_json(renamed)
        .include_function_parameter_names(true)
        .omit_blanket_impls(true)
        .build()?;
    let mut api = RustApi::default();
    for item in public.items() {
        let tokens: Vec<&Token> = item.tokens().collect();
        let (key, rendered) = (key(&tokens), item.to_string());
        if let Some(file) = files.get(&item.id().0) {
            api.declared
                .entry(file.clone())
                .or_default()
                .push((key.clone(), rendered.clone()));
        }
        api.surface.insert(key, rendered);
    }
    Ok(api)
}

/// Writes a copy of the rustdoc JSON in which the documented crate is named
/// `crate`, so the paths of paired crates with different names line up, and
/// returns it with the file declaring each item of the crate, by item id. Like
/// `public-api`, it reads the JSON without serde_json's recursion limit, which
/// deeply nested types exceed.
fn rename_crate(json: &Path) -> anyhow::Result<(PathBuf, HashMap<u32, PathBuf>)> {
    let text =
        std::fs::read_to_string(json).with_context(|| format!("reading {}", json.display()))?;
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let mut doc: serde_json::Value = serde::Deserialize::deserialize(&mut de)
        .and_then(|doc| de.end().map(|()| doc))
        .with_context(|| format!("parsing {}", json.display()))?;
    let root = doc["root"].to_string();
    doc["index"][&root]["name"] = "crate".into();
    for summary in doc["paths"]
        .as_object_mut()
        .into_iter()
        .flat_map(|p| p.values_mut())
    {
        if summary["crate_id"] == 0 {
            summary["path"][0] = "crate".into();
        }
    }
    let mut files = HashMap::new();
    for (id, item) in doc["index"].as_object().into_iter().flatten() {
        if item["crate_id"] != 0 {
            continue;
        }
        if let Some(file) = item["span"]["filename"].as_str() {
            let id = id
                .parse()
                .with_context(|| format!("{} has the item id {id}", json.display()))?;
            files.insert(id, PathBuf::from(file));
        }
    }
    let renamed = json.with_extension("crate.json");
    std::fs::write(&renamed, serde_json::to_vec(&doc)?)
        .with_context(|| format!("writing {}", renamed.display()))?;
    Ok((renamed, files))
}

fn render(tokens: &[&Token]) -> String {
    tokens.iter().map(|t| t.text()).collect()
}

fn is_path_token(t: &Token) -> bool {
    matches!(
        t,
        Token::Identifier(_)
            | Token::Type(_)
            | Token::Function(_)
            | Token::Self_(_)
            | Token::Primitive(_)
    ) || matches!(t, Token::Symbol(s) if s == "::")
}

/// `rust <self type> impl <trait>` for an impl, `rust <path> <kind>` for any
/// other item (`member` for struct fields and enum variants, which have no
/// kind). The path skips the attributes and qualifiers rendered before it,
/// and starts with a primitive for an item of an impl on one (`u8::from`).
fn key(tokens: &[&Token]) -> String {
    if matches!(tokens.first(), Some(Token::Keyword(k)) if k == "impl") {
        let (trait_part, self_part) = impl_parts(tokens);
        return format!(
            "rust {} impl {}",
            render(self_part).trim(),
            render(trait_part).trim()
        )
        .trim_end()
        .to_owned();
    }
    let kind = tokens
        .iter()
        .find_map(|t| match t {
            Token::Kind(k) => Some(k.as_str()),
            _ => None,
        })
        .unwrap_or("member");
    let start = tokens
        .iter()
        .position(|t| {
            !matches!(
                t,
                Token::Annotation(_)
                    | Token::Qualifier(_)
                    | Token::Kind(_)
                    | Token::Keyword(_)
                    | Token::Whitespace
            )
        })
        .unwrap_or(tokens.len());
    format!("rust {} {kind}", path(&tokens[start..]))
}

/// The path at the start of `tokens`, without the generic arguments of its
/// segments: `crate::Holder<'a, T>::get` is `crate::Holder::get`.
fn path(tokens: &[&Token]) -> String {
    let mut path = String::new();
    let mut depth = 0;
    for t in tokens {
        let delta = angle_delta(t);
        if depth > 0 || delta > 0 {
            depth += delta;
        } else if is_path_token(t) {
            path.push_str(t.text());
        } else {
            break;
        }
    }
    path
}

/// Splits `impl<G> Trait for Self where …` into (`Trait`, `Self`); an
/// inherent impl has an empty trait part. Generic parameters and the `where`
/// clause stay in the rendered value, not in the key.
fn impl_parts<'a>(tokens: &'a [&'a Token]) -> (&'a [&'a Token], &'a [&'a Token]) {
    let mut i = 1;
    if matches!(tokens.get(i), Some(Token::Symbol(s)) if s == "<") {
        let mut depth = 0;
        while i < tokens.len() {
            depth += angle_delta(tokens[i]);
            i += 1;
            if depth == 0 {
                break;
            }
        }
    }
    let body = &tokens[i..];
    let mut depth = 0;
    let mut for_at = None;
    let mut where_at = body.len();
    for (j, t) in body.iter().enumerate() {
        depth += angle_delta(t);
        match t {
            Token::Keyword(k) if k == "for" && depth == 0 && for_at.is_none() => for_at = Some(j),
            Token::Keyword(k) if k == "where" && depth == 0 => {
                where_at = j;
                break;
            }
            _ => {}
        }
    }
    match for_at {
        Some(f) => (&body[..f], &body[f + 1..where_at]),
        None => (&body[..0], &body[..where_at]),
    }
}

fn angle_delta(t: &Token) -> i32 {
    match t {
        Token::Symbol(s) if s != "->" => {
            s.matches('<').count() as i32 - s.matches('>').count() as i32
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::NIGHTLY;

    #[test]
    fn the_justfile_installs_the_toolchain_the_tool_runs() {
        let justfile = include_str!("../../justfile");
        assert!(
            justfile.contains(&format!("nightly := \"{NIGHTLY}\"")),
            "the justfile's nightly variable must be {NIGHTLY}:\n{justfile}"
        );
    }
}
