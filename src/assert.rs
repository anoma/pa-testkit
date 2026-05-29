//! Assertion helpers for negative integration tests, shared by every
//! integration-test crate.

/// What to look for in a failure's debug rendering.
#[derive(Debug)]
pub enum Needle {
    Static(&'static str),
    Regexp(regex::Regex),
}

/// Returns an assertion that a result is an error whose debug rendering
/// contains `needle` — used as an rstest case parameter so negative tests can
/// pin the exact check they defeat.
pub fn expect_integration_panic<T>(
    needle: Needle,
) -> impl FnOnce(anyhow::Result<T>) -> anyhow::Result<()> {
    move |result| {
        let Err(error) = result else {
            anyhow::bail!("expected to find error {needle:?}, but got anyhow::Ok");
        };

        let dbg_error = format!("{error:?}");
        let found_needle = match &needle {
            Needle::Static(s) => dbg_error.contains(s),
            Needle::Regexp(re) => re.is_match(&dbg_error),
        };

        if !found_needle {
            return Err(error.context(format!("couldn't find needle {needle:?} in error")));
        }

        Ok(())
    }
}
