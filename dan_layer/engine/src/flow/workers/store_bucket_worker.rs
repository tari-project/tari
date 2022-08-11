// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::{Arc, RwLock};

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};
use tari_common_types::types::PublicKey;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{models::Bucket, state::StateDbUnitOfWork};

pub struct StoreBucketWorker<TUnitOfWork: StateDbUnitOfWork> {
    pub state_db: Arc<RwLock<TUnitOfWork>>,
}

impl<TUnitOfWork: StateDbUnitOfWork> Worker for StoreBucketWorker<TUnitOfWork> {
    // fn call(&self, node: Node, inputs: InputData) -> OutputData {
    //     let mut map = HashMap::new();
    //     let bucket: Bucket = match node.get_field_t("bucket", &inputs) {
    //         Ok(a) => a,
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //
    //     let to = match node.get_string_field("to", &inputs) {
    //         Ok(a) => PublicKey::from_hex(&a).expect("Not a valid pub key"),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //
    //     let mut state = self.state_db.write().unwrap();
    //     let balance_key = format!("token_id-{}-{}", bucket.asset_id(), bucket.token_id());
    //     let balance = match state.get_u64(&balance_key, to.as_bytes()) {
    //         Ok(b) => b.unwrap_or(0),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err.into()));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     match state.set_u64(
    //         &balance_key,
    //         to.as_bytes(),
    //         bucket.amount().checked_add(balance).expect("overflowed"),
    //     ) {
    //         Ok(_) => (),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err.into()));
    //             return Rc::new(err_map);
    //         },
    //     }
    //
    //     map.insert("default".to_string(), Ok(IOData { data: Box::new(()) }));
    //     // map.insert("bucket".to_string(), Ok(IOData { data: Box::new(bucket) }));
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "tari::store_bucket"
    }

    fn work(&self, node: &Node, inputs: InputData) -> anyhow::Result<OutputData> {
        let bucket: Bucket = serde_json::from_str(&node.get_string_field("bucket", &inputs)?)?;
        let to = PublicKey::from_hex(&node.get_string_field("to", &inputs)?)?;
        let mut state = self.state_db.write().unwrap();
        // TODO: handle panics
        let balance_key = format!(
            "token_id-{}-{}",
            bucket.resource_address(),
            bucket.token_ids().unwrap()[0]
        );
        let balance = state.get_u64(&balance_key, to.as_bytes())?.unwrap_or(0);
        state.set_u64(
            &balance_key,
            to.as_bytes(),
            bucket.amount().checked_add(balance).expect("overflowed"),
        )?;

        let output = OutputDataBuilder::new().data("default", Box::new(())).build();
        Ok(output)
    }
}
