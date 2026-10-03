use std::path::PathBuf;
use std::process::Command;

use public_api::tokens::Token;

use crate::compare::Surface;
use crate::packages::Package;

/// The toolchain whose rustdoc JSON `public-api` 0.52.2 reads.
pub const NIGHTLY: &str = "nightly-2026-02-08";

fn rustup(args: &[&str]) -> Result<String, String> {
    let out = Command::new("rustup")
        .args(args)
        .output()
        .map_err(|e| format!("running rustup: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "rustup {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The pinned toolchain's `rustc` or `rustdoc`. A missing toolchain is an
/// error: naming it to rustup would install it, and concurrent installs of
/// one toolchain fail.
fn nightly_binary(name: &str) -> Result<PathBuf, String> {
    let installed = rustup(&["toolchain", "list"])?;
    if !installed
        .lines()
        .any(|l| l.starts_with(&format!("{NIGHTLY}-")))
    {
        return Err(format!(
            "toolchain {NIGHTLY} is not installed; install it with `rustup toolchain install {NIGHTLY} --profile minimal`"
        ));
    }
    rustup(&["which", "--toolchain", NIGHTLY, name]).map(PathBuf::from)
}

/// The public API of the package's library target, built with all features.
/// A package without a library target exposes no Rust items.
pub fn surface(pkg: &Package) -> Result<Surface, String> {
    let Some(crate_name) = &pkg.lib_name else {
        return Ok(Surface::default());
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    // rustdoc JSON only loads dependencies compiled by the same rustc, so both
    // binaries come from the pinned toolchain whatever PATH resolves first.
    let json = rustdoc_json::Builder::default()
        .toolchain(NIGHTLY)
        .env("RUSTC", nightly_binary("rustc")?)
        .env("RUSTDOC", nightly_binary("rustdoc")?)
        .manifest_path(&pkg.manifest)
        .package(&pkg.name)
        .all_features(true)
        .build_with_captured_output(&mut stdout, &mut stderr)
        .map_err(|e| format!("{e}\n{}", String::from_utf8_lossy(&stderr)))?;
    let api = public_api::Builder::from_rustdoc_json(json)
        .include_function_parameter_names(true)
        .build()
        .map_err(|e| e.to_string())?;
    let mut s = Surface::default();
    for item in api.items() {
        let tokens: Vec<&Token> = item.tokens().collect();
        let start = path_start(&tokens);
        s.insert(
            key(&tokens, start, crate_name),
            render(&tokens, Some(start), crate_name),
        );
    }
    Ok(s)
}

/// The tokens' text, with the crate's own name replaced by `crate` wherever
/// it starts a path, so paired crates with different names line up. The
/// item's own path starts at `item_path`; there the crate name is replaced
/// even with nothing after it, which is how the crate root renders.
fn render(tokens: &[&Token], item_path: Option<usize>, crate_name: &str) -> String {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| match t {
            Token::Identifier(name)
                if name == crate_name
                    && (item_path == Some(i)
                        || matches!(tokens.get(i + 1), Some(Token::Symbol(s)) if s == "::")) =>
            {
                "crate"
            }
            t => t.text(),
        })
        .collect()
}

/// Where an item's own path starts, after its qualifiers and kind.
fn path_start(tokens: &[&Token]) -> usize {
    tokens
        .iter()
        .position(|t| {
            !matches!(
                t,
                Token::Qualifier(_) | Token::Kind(_) | Token::Keyword(_) | Token::Whitespace
            )
        })
        .unwrap_or(tokens.len())
}

fn is_path_token(t: &Token) -> bool {
    matches!(
        t,
        Token::Identifier(_) | Token::Type(_) | Token::Function(_) | Token::Self_(_)
    ) || matches!(t, Token::Symbol(s) if s == "::")
}

/// `rust <self type> impl <trait>` for an impl, `rust <path> <kind>` for any
/// other item (`member` for struct fields and enum variants, which have no kind).
fn key(tokens: &[&Token], start: usize, crate_name: &str) -> String {
    if matches!(tokens.first(), Some(Token::Keyword(k)) if k == "impl") {
        let (trait_part, self_part) = impl_parts(tokens);
        return format!(
            "rust {} impl {}",
            render(self_part, None, crate_name).trim(),
            render(trait_part, None, crate_name).trim()
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
    let end = tokens[start..]
        .iter()
        .position(|t| !is_path_token(t))
        .map_or(tokens.len(), |n| start + n);
    format!(
        "rust {} {kind}",
        render(&tokens[start..end], Some(0), crate_name)
    )
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
