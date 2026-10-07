//! Generated bindings compare as the ABI they were generated from. A module
//! `forge bind` writes is a function of the contract's JSON ABI and bytecode,
//! which its documentation and statics embed: thousands of Rust items that
//! carry nothing the ABI does not. A module the pinned `forge` reproduces
//! byte for byte from those embedded inputs gives way to its ABI's entries
//! (`forge bind <module> <entry>`), so nothing it publishes goes unseen. Any
//! other module, and every module of a repository pinning no Foundry, keeps
//! its Rust items and is noted.

use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, bail, ensure};
use serde_json::Value;

use crate::compare::Surface;
use crate::{cmd, interface};

/// A module `forge bind` wrote, with the inputs it embeds: the contract's
/// name, its JSON ABI, and its creation and runtime bytecode where present.
#[derive(Clone, Debug)]
pub struct ForgeBindModule {
    pub text: String,
    pub contract: String,
    pub abi: Value,
    pub bytecode: Option<Vec<u8>>,
    pub deployed_bytecode: Option<Vec<u8>>,
}

/// The `forge bind` module in `path`, or `None` for a file that embeds no
/// JSON ABI.
pub fn parse(path: &Path) -> anyhow::Result<Option<ForgeBindModule>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let Some(abi) = interface::forge_bind_abi(&text) else {
        return Ok(None);
    };
    let abi = abi.with_context(|| format!("reading the ABI in {}", path.display()))?;
    // The ABI documents the contract's module, the first one after it.
    let contract = text
        .split_once("JSON ABI:")
        .and_then(|(_, rest)| rest.split_once("\npub mod "))
        .and_then(|(_, rest)| rest.split_once(' '))
        .map(|(name, _)| name.to_owned())
        .with_context(|| format!("{} has no module after its JSON ABI", path.display()))?;
    let bytecode = static_bytes(&text, "BYTECODE")
        .with_context(|| format!("reading BYTECODE in {}", path.display()))?;
    let deployed_bytecode = static_bytes(&text, "DEPLOYED_BYTECODE")
        .with_context(|| format!("reading DEPLOYED_BYTECODE in {}", path.display()))?;
    Ok(Some(ForgeBindModule {
        text,
        contract,
        abi,
        bytecode,
        deployed_bytecode,
    }))
}

/// The bytes of `forge bind`'s `pub static <name>: Bytes`, written as a Rust
/// byte string literal, or `None` if the module has no such static.
fn static_bytes(text: &str, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let start = format!(
        "pub static {name}: alloy_sol_types::private::Bytes = alloy_sol_types::private::Bytes::from_static(\n"
    );
    let Some((_, after)) = text.split_once(&start) else {
        return Ok(None);
    };
    let literal = after
        .trim_start()
        .strip_prefix("b\"")
        .context("the static is not a byte string literal")?;
    let mut bytes = Vec::new();
    let mut chars = literal.chars();
    loop {
        match chars.next().context("the byte string is not closed")? {
            '"' => return Ok(Some(bytes)),
            '\\' => match chars.next().context("the byte string ends in an escape")? {
                'x' => {
                    let hex: String = chars.by_ref().take(2).collect();
                    bytes.push(u8::from_str_radix(&hex, 16).context("a malformed \\x escape")?);
                }
                'n' => bytes.push(b'\n'),
                'r' => bytes.push(b'\r'),
                't' => bytes.push(b'\t'),
                '0' => bytes.push(0),
                '\\' => bytes.push(b'\\'),
                '"' => bytes.push(b'"'),
                '\'' => bytes.push(b'\''),
                other => bail!("an unknown escape \\{other}"),
            },
            c => {
                ensure!(c.is_ascii(), "a byte string holds only ASCII, not {c:?}");
                bytes.push(c as u8);
            }
        }
    }
}

/// Whether `forge` writes exactly `module` from the ABI and bytecode it
/// embeds.
pub fn reproduces(module: &ForgeBindModule, forge: &Path) -> anyhow::Result<bool> {
    let project = scratch_project()?;
    let reproduced = bind(module, forge, &project);
    std::fs::remove_dir_all(&project).with_context(|| format!("removing {}", project.display()))?;
    reproduced
}

/// A new empty directory for one `forge bind` project, in the system's
/// temporary directory: `forge bind` finds no artifacts in a project below
/// a directory named `target` (observed with Foundry v1.8.5), as the tool's
/// work directory is.
fn scratch_project() -> anyhow::Result<PathBuf> {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let project = std::env::temp_dir().join(format!(
        "interface-parity-forge-bind-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    ensure!(
        !project.components().any(|c| c.as_os_str() == "target"),
        "{} is below a directory named target, where forge bind finds no artifacts",
        project.display()
    );
    cmd::fresh_dir(&project)?;
    Ok(project)
}

/// Writes `module`'s contract as a Foundry artifact in `project`, runs
/// `forge bind` on it, and compares what it writes with the module.
fn bind(module: &ForgeBindModule, forge: &Path, project: &Path) -> anyhow::Result<bool> {
    let mut artifact = serde_json::json!({ "abi": module.abi });
    for (key, code) in [
        ("bytecode", &module.bytecode),
        ("deployedBytecode", &module.deployed_bytecode),
    ] {
        if let Some(code) = code {
            artifact[key] = serde_json::json!({ "object": format!("0x{}", hex::encode(code)) });
        }
    }
    let out = project.join("out").join(format!("{}.sol", module.contract));
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    std::fs::write(
        out.join(format!("{}.json", module.contract)),
        serde_json::to_vec(&artifact)?,
    )?;
    std::fs::write(
        project.join("foundry.toml"),
        "[profile.default]\nsrc = \"src\"\nout = \"out\"\n",
    )?;
    std::fs::create_dir_all(project.join("src"))?;
    cmd::stdout(
        Command::new(forge)
            .current_dir(project)
            .args(["bind", "--skip-build", "--module", "--overwrite"])
            .args(["--bindings-path", "bindings", "--select"])
            .arg(format!("^{}$", module.contract)),
    )?;
    let written = std::fs::read_dir(project.join("bindings"))?
        .map(|entry| Ok(entry?.path()))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n != "mod.rs"))
        .collect::<Vec<_>>();
    let [file] = &written[..] else {
        bail!(
            "forge bind wrote {written:?}, not one module for {}",
            module.contract
        );
    };
    Ok(std::fs::read_to_string(file)? == module.text)
}

/// The Rust module path of `file`, a source file of the crate rooted at
/// `src` (the directory holding `lib.rs`).
pub fn module_path(src: &Path, file: &Path) -> anyhow::Result<String> {
    let relative = file
        .strip_prefix(src)
        .with_context(|| format!("{} is outside {}", file.display(), src.display()))?;
    let mut path = vec!["crate".to_owned()];
    for component in relative.with_extension("").components() {
        match component {
            Component::Normal(name) => path.push(name.to_string_lossy().into_owned()),
            other => bail!("{} has the path component {other:?}", file.display()),
        }
    }
    if matches!(path.last().map(String::as_str), Some("lib" | "mod")) {
        path.pop();
    }
    Ok(path.join("::"))
}

/// Replaces the Rust items of `module`, the `forge bind` module at the Rust
/// path `path`, with the entries of the ABI it was generated from, the
/// contract it binds and which bytecode it carries. A path with no items in
/// `surface` is an error: the module would compare as nothing at all.
pub fn collapse(surface: &mut Surface, path: &str, module: &ForgeBindModule) -> anyhow::Result<()> {
    let abi = interface::abi_surface(&module.abi)?;
    ensure!(
        surface.remove_rust_module(path) > 0,
        "the crate's API has no items under {path}, the module forge bind wrote"
    );
    let key = |entry: &str| format!("forge bind {path} {entry}");
    surface.insert(key("contract"), module.contract.clone());
    for (name, code) in [
        ("creation", &module.bytecode),
        ("runtime", &module.deployed_bytecode),
    ] {
        if code.is_some() {
            surface.insert(key("bytecode"), name);
        }
    }
    for (entry, values) in abi.entries() {
        for value in values {
            surface.insert(key(entry), value.clone());
        }
    }
    Ok(())
}

/// Collapses every `forge bind` module of the crate rooted at `src` that
/// `forge` reproduces, and returns a note for each module left item by item:
/// one `forge` does not reproduce, or, with no pinned `forge`, every module.
pub fn collapse_crate(
    surface: &mut Surface,
    src: &Path,
    forge: Option<&Path>,
) -> anyhow::Result<Vec<String>> {
    let mut notes = vec![];
    for file in rust_files(src)? {
        let Some(module) = parse(&file)? else {
            continue;
        };
        let path = module_path(src, &file)?;
        match forge {
            None => notes.push(format!(
                "{path} is a forge bind module, compared item by item: the repository pins no \
                 Foundry release to reproduce it from its ABI"
            )),
            Some(forge) if reproduces(&module, forge)? => collapse(surface, &path, &module)?,
            Some(forge) => notes.push(format!(
                "{path} is not what {} writes from its embedded ABI and bytecode, so it is \
                 compared item by item",
                forge_version(forge)?
            )),
        }
    }
    Ok(notes)
}

/// `forge --version`'s first line.
fn forge_version(forge: &Path) -> anyhow::Result<String> {
    let out = cmd::stdout(Command::new(forge).arg("--version"))?;
    Ok(out.lines().next().unwrap_or_default().trim().to_owned())
}

/// Every `.rs` file under `dir`, sorted.
fn rust_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = vec![];
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(rust_files(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
