const ALICE: &str = "7ad4b84636a3fa408827e7202f6da39287bbf099d1fab6250d3b56e03e77586b";

const BOB: &str = "59c6995e998f97a5a0044966f094538d8f0f1d1593f17f5f8cfb743f7f6f5f17";

pub use keychain::{Keychain, alice, bob};

mod keychain {
    use anoma_rm_risc0::nullifier_key::NullifierKey;
    use anoma_rm_risc0_gadgets::authority::AuthoritySigningKey;
    use anoma_rm_risc0_gadgets::authority::AuthorityVerifyingKey;
    use anyhow::Context;
    use k256::AffinePoint;

    use super::{ALICE, BOB};

    /// A test participant's identity: the ARM key material plus a secp256k1
    /// account secret. EVM-specific accessors (`signer` / `address`) are added in
    /// the pa-evm repo through an extension trait, so the alloy signer
    /// type never enters the testkit.
    #[derive(Clone)]
    pub struct Keychain {
        /// secp256k1 account secret key (on EVM, the signer's private key).
        pub signing_key: [u8; 32],
        pub auth_signing_key: AuthoritySigningKey,
        pub nf_key: NullifierKey,
        pub discovery_pk: AffinePoint,
        pub encryption_pk: AffinePoint,
    }

    impl Keychain {
        #[inline]
        pub fn auth_verifying_key(&self) -> AuthorityVerifyingKey {
            AuthorityVerifyingKey::from_signing_key(&self.auth_signing_key)
        }
    }

    pub fn alice() -> anyhow::Result<Keychain> {
        Ok(Keychain {
            signing_key: secret_key(ALICE)?,
            auth_signing_key: bincode::deserialize(&[
                49, 163, 242, 139, 6, 69, 133, 86, 182, 239, 39, 243, 37, 180, 9, 187, 61, 164,
                247, 146, 159, 90, 229, 55, 22, 194, 229, 195, 98, 53, 192, 188,
            ])?,
            nf_key: bincode::deserialize(&[
                120, 99, 45, 87, 215, 212, 105, 151, 232, 133, 145, 106, 219, 2, 222, 47, 44, 229,
                111, 197, 157, 203, 215, 1, 27, 157, 132, 58, 155, 86, 16, 121,
            ])?,
            discovery_pk: bincode::deserialize(&[
                33, 0, 0, 0, 0, 0, 0, 0, 3, 46, 104, 140, 146, 240, 157, 80, 158, 161, 213, 176,
                129, 104, 105, 100, 4, 188, 61, 218, 24, 133, 31, 110, 105, 62, 201, 6, 235, 81,
                22, 136, 9,
            ])?,
            encryption_pk: bincode::deserialize(&[
                33, 0, 0, 0, 0, 0, 0, 0, 2, 89, 92, 12, 22, 199, 233, 162, 216, 222, 105, 146, 154,
                50, 232, 161, 210, 135, 150, 63, 178, 234, 21, 129, 202, 93, 250, 5, 43, 21, 62,
                145, 159,
            ])?,
        })
    }

    pub fn bob() -> anyhow::Result<Keychain> {
        Ok(Keychain {
            signing_key: secret_key(BOB)?,
            auth_signing_key: bincode::deserialize(&[
                194, 126, 0, 187, 29, 229, 72, 225, 199, 156, 11, 62, 167, 64, 148, 184, 132, 28,
                88, 85, 247, 61, 178, 71, 59, 145, 47, 88, 105, 188, 68, 2,
            ])?,
            nf_key: bincode::deserialize(&[
                212, 193, 211, 83, 212, 166, 3, 198, 134, 227, 35, 142, 179, 45, 136, 41, 194, 3,
                100, 221, 222, 130, 3, 218, 246, 196, 162, 40, 53, 59, 130, 98,
            ])?,
            discovery_pk: bincode::deserialize(&[
                33, 0, 0, 0, 0, 0, 0, 0, 3, 59, 151, 4, 148, 81, 229, 17, 59, 101, 124, 243, 133,
                207, 118, 88, 36, 114, 138, 211, 56, 56, 63, 14, 191, 195, 241, 58, 144, 38, 223,
                80, 194,
            ])?,
            encryption_pk: bincode::deserialize(&[
                33, 0, 0, 0, 0, 0, 0, 0, 2, 178, 56, 160, 238, 16, 199, 140, 94, 215, 48, 45, 177,
                157, 249, 90, 214, 36, 214, 152, 41, 227, 220, 80, 14, 71, 140, 34, 49, 75, 234,
                244, 172,
            ])?,
        })
    }

    fn secret_key(hex_key: &str) -> anyhow::Result<[u8; 32]> {
        hex::decode(hex_key)
            .context("invalid hex account key")?
            .as_slice()
            .try_into()
            .context("account key must be 32 bytes")
    }
}
