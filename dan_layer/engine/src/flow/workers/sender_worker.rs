// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};
use tari_common_types::types::PublicKey;
use tari_utilities::hex::Hex;

pub struct SenderWorker {
    pub sender: PublicKey,
}

impl Worker for SenderWorker {
    // fn call(&self, _node: Node, _input: InputData) -> OutputData {
    //     let mut map = HashMap::new();
    //     map.insert(
    //         "default".to_string(),
    //         Ok(IOData {
    //             data: Box::new(self.sender.to_hex()),
    //         }),
    //     );
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "core::sender"
    }

    fn work(&self, _node: &Node, _input_data: InputData) -> anyhow::Result<OutputData> {
        Ok(OutputDataBuilder::new()
            .data("default", Box::new(self.sender.to_hex()))
            .build())
    }
}
