use tari_common_types::types::{PublicKey, Signature};
use tari_core::transactions::transaction_components::{ValidatorNodeRegistration, ValidatorNodeSignature};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_utilities::ByteArray;

use crate::conversions::error::ConversionError;

impl From<&ValidatorNodeRegistration> for crate::tari_rpc::ValidatorNodeRegistration {
    fn from(registration: &ValidatorNodeRegistration) -> Self {
        Self {
            public_key: registration.public_key().to_vec(),
            signature: Some(crate::tari_rpc::Signature {
                public_nonce: registration.signature().get_public_nonce().to_vec(),
                signature: registration.signature().get_signature().to_vec(),
            }),
            claim_public_key: registration.claim_public_key().to_vec(),
            sidechain_id: match registration.sidechain_id() {
                None => vec![],
                Some(id) => id.to_vec(),
            },
            sidechain_id_knowledge_proof: registration.sidechain_id_knowledge_proof().map(|signature| {
                crate::tari_rpc::Signature {
                    public_nonce: signature.get_public_nonce().to_vec(),
                    signature: signature.get_signature().to_vec(),
                }
            }),
        }
    }
}

impl TryFrom<crate::tari_rpc::Signature> for Signature {
    type Error = ConversionError;

    fn try_from(sig: crate::tari_rpc::Signature) -> Result<Self, Self::Error> {
        Ok(Self::new(
            PublicKey::from_canonical_bytes(sig.public_nonce.as_slice()).map_err(ConversionError::PublicKey)?,
            RistrettoSecretKey::from_canonical_bytes(sig.signature.as_slice()).map_err(ConversionError::SecretKey)?,
        ))
    }
}

impl TryFrom<crate::tari_rpc::ValidatorNodeRegistration> for ValidatorNodeRegistration {
    type Error = ConversionError;

    fn try_from(reg: crate::tari_rpc::ValidatorNodeRegistration) -> Result<Self, Self::Error> {
        let reg_signature = reg.signature.ok_or(ConversionError::MissingField("signature"))?;
        let signature = ValidatorNodeSignature::new(
            PublicKey::from_canonical_bytes(reg_signature.public_nonce.as_slice())
                .map_err(ConversionError::PublicKey)?,
            reg_signature.try_into()?,
        );
        let sidechain_id = if reg.sidechain_id.is_empty() {
            None
        } else {
            Some(PublicKey::from_canonical_bytes(reg.sidechain_id.as_slice()).map_err(ConversionError::PublicKey)?)
        };
        Ok(Self::new(
            signature,
            PublicKey::from_canonical_bytes(reg.claim_public_key.as_slice()).map_err(ConversionError::PublicKey)?,
            sidechain_id,
            match reg.sidechain_id_knowledge_proof {
                None => None,
                Some(signature) => signature.try_into()?,
            },
        ))
    }
}
