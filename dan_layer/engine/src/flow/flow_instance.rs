// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, RwLock},
};

use d3ne::{Engine, Node, Workers, WorkersBuilder};
use serde_json::Value as JsValue;
use tari_common_types::types::PublicKey;
use tari_utilities::ByteArray;

use crate::{
    flow::{
        workers::{
            ArgWorker,
            CreateBucketWorker,
            HasRoleWorker,
            MintBucketWorker,
            SenderWorker,
            StartWorker,
            StoreBucketWorker,
            TextWorker,
        },
        ArgValue,
        FlowEngineError,
    },
    function_definitions::{ArgType, FunctionArgDefinition},
    state::StateDbUnitOfWork,
};

#[derive(Clone, Debug)]
pub struct FlowInstance {
    // engine: Engine,
    // TODO: engine is not Send so can't be added here
    // process: JsValue,
    start_node: i64,
    nodes: HashMap<i64, Node>,
}

impl FlowInstance {
    pub fn try_build(value: JsValue, workers: Workers) -> Result<Self, FlowEngineError> {
        let engine = Engine::new("tari@0.1.0", workers);
        // dbg!(&value);
        let nodes = engine.parse_value(value).expect("could not create engine");
        Ok(FlowInstance {
            // process: value,
            nodes,
            start_node: 1,
        })
    }

    pub fn process<TUnitOfWork: StateDbUnitOfWork + 'static>(
        &self,
        args: &[u8],
        arg_defs: &[FunctionArgDefinition],
        sender: PublicKey,
        state_db: TUnitOfWork,
    ) -> Result<TUnitOfWork, FlowEngineError> {
        let mut engine_args = HashMap::new();

        let mut remaining_args = Vec::from(args);
        for ad in arg_defs {
            let value = match ad.arg_type {
                ArgType::String => {
                    let length = remaining_args.pop().expect("no more args: len") as usize;
                    let s_bytes: Vec<u8> = remaining_args.drain(0..length).collect();
                    let s = String::from_utf8(s_bytes).expect("could not convert string");
                    ArgValue::String(s)
                },
                ArgType::Byte => ArgValue::Byte(remaining_args.pop().expect("No byte to read")),
                ArgType::PublicKey => {
                    let bytes: Vec<u8> = remaining_args.drain(0..32).collect();
                    let pk = PublicKey::from_bytes(&bytes).expect("Not a valid public key");
                    ArgValue::PublicKey(pk)
                },
                ArgType::Uint => {
                    let bytes: Vec<u8> = remaining_args.drain(0..8).collect();
                    let mut fixed: [u8; 8] = [0u8; 8];
                    fixed.copy_from_slice(&bytes);
                    let value = u64::from_le_bytes(fixed);
                    ArgValue::Uint(value)
                },
            };
            engine_args.insert(ad.name.clone(), value);
        }

        let state_db = Arc::new(RwLock::new(state_db));
        let engine = Engine::new("tari@0.1.0", load_workers(engine_args, sender, state_db.clone()));
        let output = engine.process(&self.nodes, self.start_node);
        let _od = output.expect("engine process failed");
        // if let Some(err) = od.get("error") {
        //     match err {
        //         Ok(_) => todo!("Unexpected Ok result returned from error"),
        //         Err(e) => {
        //             return Err(FlowEngineError::InstructionFailed { inner: e.to_string() });
        //         },
        //     }
        // }
        let inner = state_db.read().map(|s| s.deref().clone()).unwrap();
        Ok(inner)
    }
}

fn load_workers<TUnitOfWork: StateDbUnitOfWork + 'static>(
    args: HashMap<String, ArgValue>,
    sender: PublicKey,
    state_db: Arc<RwLock<TUnitOfWork>>,
) -> Workers {
    let mut workers = WorkersBuilder::new();
    workers.add(StartWorker {});
    workers.add(CreateBucketWorker {
        state_db: state_db.clone(),
    });
    workers.add(StoreBucketWorker {
        state_db: state_db.clone(),
    });
    workers.add(ArgWorker { args: args.clone() });
    workers.add(ArgWorker { args });
    workers.add(SenderWorker { sender });
    workers.add(TextWorker {});
    workers.add(HasRoleWorker { state_db });
    workers.add(MintBucketWorker {});
    workers.build()
}
