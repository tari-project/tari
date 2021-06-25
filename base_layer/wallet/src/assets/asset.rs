use tari_core::transactions::types::{PublicKey, Commitment};

#[derive(Clone)]
pub struct Asset {
    name : String,
    registration_output_status: String,
    public_key: PublicKey,
    owner_commitment: Commitment
}

impl Asset {
    pub fn new(name: String, registration_output_status: String, public_key: PublicKey, owner_commitment: Commitment)  -> Self{
       Self {
           name,
           registration_output_status,
           public_key,
           owner_commitment
       }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn registration_output_status(&self) -> &str {
        self.registration_output_status.as_str()
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn owner_commitment(&self) -> &Commitment {
        &self.owner_commitment
    }
}
