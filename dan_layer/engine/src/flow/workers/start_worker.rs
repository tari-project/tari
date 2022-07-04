// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};

pub struct StartWorker {}

impl Worker for StartWorker {
    // fn call(&self, _node: Node, _inputs: InputData) -> OutputData {
    //     let mut map = HashMap::new();
    //     map.insert("default".to_string(), Ok(IOData { data: Box::new(()) }));
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "core::start"
    }

    fn work(&self, _node: &Node, _input_data: InputData) -> anyhow::Result<OutputData> {
        Ok(OutputDataBuilder::new().data("default", Box::new(())).build())
    }
}
