// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::TryFrom;

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};

use crate::models::{Bucket, ResourceAddress};

pub struct MintBucketWorker {}

impl Worker for MintBucketWorker {
    // fn call(&self, node: Node, inputs: InputData) -> OutputData {
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
    //
    //     let asset_id = match node.get_number_field("asset_id", &inputs) {
    //         Ok(a) => a as u64,
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //     let bucket = Bucket::new(amount, token_id, asset_id);
    //     map.insert("default".to_string(), Ok(IOData { data: Box::new(()) }));
    //     map.insert("bucket".to_string(), Ok(IOData { data: Box::new(bucket) }));
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "tari::mint_bucket"
    }

    fn work(&self, node: &Node, inputs: InputData) -> anyhow::Result<OutputData> {
        let _amount = u64::try_from(node.get_number_field("amount", &inputs)?)?;
        let token_id = u64::try_from(node.get_number_field("token_id", &inputs)?)?;
        let vault_id = ResourceAddress::from_hex(&node.get_string_field("vault_id", &inputs)?)?;
        let bucket = Bucket::for_token(vault_id, vec![token_id]);
        let output = OutputDataBuilder::new()
            .data("default", Box::new(()))
            .data("bucket", Box::new(bucket))
            .build();
        Ok(output)
    }
}
