// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    sync::{Arc, RwLock},
};

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};
use tari_common_types::types::PublicKey;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    models::{Bucket, ResourceAddress},
    state::StateDbUnitOfWork,
};

pub struct CreateBucketWorker<TUnitOfWork: StateDbUnitOfWork> {
    pub state_db: Arc<RwLock<TUnitOfWork>>,
}

impl<TUnitOfWork: StateDbUnitOfWork> Worker for CreateBucketWorker<TUnitOfWork> {
    fn name(&self) -> &str {
        "tari::create_bucket"
    }

    fn work(&self, node: &Node, input_data: InputData) -> anyhow::Result<OutputData> {
        // TODO: return proper errors....
        let amount = u64::try_from(node.get_number_field("amount", &input_data)?)?;
        let vault_id = ResourceAddress::from_hex(&node.get_string_field("vault_id", &input_data)?)?;
        let token_id = u64::try_from(node.get_number_field("token_id", &input_data)?)?;
        let from = PublicKey::from_hex(&node.get_string_field("from", &input_data)?).expect("Not a valid pub key");
        let mut state = self.state_db.write().unwrap();
        let balance_key = format!("token_id-{}-{}", vault_id, token_id);
        let balance = state.get_u64(&balance_key, from.as_bytes())?.unwrap_or(0);
        let new_balance = balance.checked_sub(amount).expect("Not enough funds to create bucket");
        state
            .set_u64(&balance_key, from.as_bytes(), new_balance)
            .expect("Could not save state");
        let output = OutputDataBuilder::new()
            .data("default", Box::new(()))
            .data("bucket", Box::new(Bucket::for_token(vault_id, vec![token_id])))
            .build();
        Ok(output)
    }
    //     let mut map = HashMap::new();
    //     let amount = match node.get_number_field("amount", &inputs) {
    //         Ok(a) => a as u64,
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     let token_id = match node.get_number_field("token_id", &inputs) {
    //         Ok(a) => a as u64,
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     let asset_id = match node.get_number_field("asset_id", &inputs) {
    //         Ok(a) => a as u64,
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     let from = match node.get_string_field("from", &inputs) {
    //         Ok(a) => PublicKey::from_hex(&a).expect("Not a valid pub key"),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //
    //     let mut state = self.state_db.write().unwrap();
    //     let balance_key = format!("token_id-{}-{}", asset_id, token_id);
    //     let balance = match state.get_u64(&balance_key, from.as_bytes()) {
    //         Ok(b) => b.unwrap_or(0),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err.into()));
    //             return Rc::new(err_map);
    //         },
    //     };
    //
    //     let new_balance = match balance.checked_sub(amount) {
    //         Some(x) => x,
    //         None => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(anyhow!("Not enough funds to create bucket")));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     match state.set_u64(&balance_key, from.as_bytes(), new_balance) {
    //         Ok(_) => (),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err.into()));
    //             return Rc::new(err_map);
    //         },
    //     }
    //
    //     let bucket = Bucket::new(amount, token_id, asset_id);
    //     map.insert("default".to_string(), Ok(IOData { data: Box::new(()) }));
    //     map.insert("bucket".to_string(), Ok(IOData { data: Box::new(bucket) }));
    //     Rc::new(map)
    // }
}
