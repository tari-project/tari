// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::{Arc, RwLock};

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};

use crate::state::StateDbUnitOfWork;

pub struct HasRoleWorker<TUnitOfWork: StateDbUnitOfWork> {
    pub state_db: Arc<RwLock<TUnitOfWork>>,
}
impl<TUnitOfWork: StateDbUnitOfWork> Worker for HasRoleWorker<TUnitOfWork> {
    // fn call(&self, node: Node, inputs: InputData) -> OutputData {
    //     let _role = node.get_string_field("role", &inputs).unwrap();
    //
    //     let _pubkey = match node.get_string_field("pubkey", &inputs) {
    //         Ok(a) => PublicKey::from_hex(&a).expect("Not a valid pub key"),
    //         Err(err) => {
    //             let mut err_map = HashMap::new();
    //             err_map.insert("error".to_string(), Err(err));
    //             return Rc::new(err_map);
    //         },
    //     };
    //
    //     // let state = self.state_db.read().expect("Could not get lock on data");
    //     // state.get_value()
    //     // TODO: read roles from db
    //     let mut map = HashMap::new();
    //     map.insert("default".to_string(), Ok(IOData { data: Box::new(()) }));
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "tari::has_role"
    }

    fn work(&self, node: &Node, input_data: InputData) -> anyhow::Result<OutputData> {
        let _role = node.get_string_field("role", &input_data)?;
        Ok(OutputDataBuilder::new().data("default", Box::new(())).build())
    }
}
