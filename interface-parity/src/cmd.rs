use std::io::Write;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, bail};

/// Runs `cmd` and returns its stdout. A failure to start, a non-zero exit or
/// non-UTF-8 output is an error carrying the command line and its stderr.
pub fn stdout(cmd: &mut Command) -> anyhow::Result<String> {
    let out = cmd.output().with_context(|| format!("running {cmd:?}"))?;
    checked(cmd, out)
}

/// Like [`stdout`], with `input` written to the command's stdin.
pub fn stdout_with_input(cmd: &mut Command, input: &[u8]) -> anyhow::Result<String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("running {cmd:?}"))?;
    child
        .stdin
        .take()
        .context("stdin was not piped")?
        .write_all(input)
        .with_context(|| format!("writing the stdin of {cmd:?}"))?;
    let out = child
        .wait_with_output()
        .with_context(|| format!("waiting for {cmd:?}"))?;
    checked(cmd, out)
}

fn checked(cmd: &Command, out: Output) -> anyhow::Result<String> {
    if !out.status.success() {
        bail!(
            "{cmd:?} failed ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).with_context(|| format!("{cmd:?} printed non-UTF-8 output"))
}
