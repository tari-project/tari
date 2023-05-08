use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct UiBurntProof {
    pub id: u32,
    pub reciprocal_claim_public_key: String,
    pub payload: String,
    pub burned_at: NaiveDateTime,
}
