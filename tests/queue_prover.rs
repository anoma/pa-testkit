//! The queue prover without ABI encoding, which the proving queue cannot decode.
//!
//! Gated on `e2e` without `abi_encoding`; run with `cargo test --features e2e`.
#![cfg(all(feature = "e2e", feature = "fixtures", not(feature = "abi_encoding")))]

use anoma_pa_testkit::environment::Prover;
use anoma_pa_testkit::fixtures::{action, trivial};
use anoma_pa_testkit::prover::QueueProver;

#[tokio::test]
async fn prove_errors_without_abi_encoding_before_it_contacts_the_queue() -> anyhow::Result<()> {
    // No server listens on the discard port, so a request would fail with a connection error.
    let prover = QueueProver::new("http://127.0.0.1:9", "unused")?;
    let actions = action::build([
        trivial::consumed(1, trivial::Overrides::default()),
        trivial::created(),
    ])?;

    let err = prover
        .prove(&[&actions])
        .await
        .err()
        .expect("the queue prover must refuse to prove without ABI encoding");
    assert!(
        format!("{err:#}").contains("turn on the `abi_encoding` feature"),
        "{err:#}"
    );
    Ok(())
}
