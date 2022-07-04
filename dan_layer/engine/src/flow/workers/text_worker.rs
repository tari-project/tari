// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use d3ne::{InputData, Node, OutputData, OutputDataBuilder, Worker};

pub struct TextWorker {}

impl Worker for TextWorker {
    // fn call(&self, node: Node, inputs: InputData) -> OutputData {
    //     let txt = node.get_string_field("txt", &inputs).unwrap();
    //     let mut map = HashMap::new();
    //     map.insert("txt".to_string(), Ok(IOData { data: Box::new(txt) }));
    //     Rc::new(map)
    // }

    fn name(&self) -> &str {
        "core::text"
    }

    fn work(&self, node: &Node, input_data: InputData) -> anyhow::Result<OutputData> {
        let txt = node.get_string_field("txt", &input_data)?;
        Ok(OutputDataBuilder::new().data("txt", Box::new(txt)).build())
    }
}
