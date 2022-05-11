use std::collections::HashMap;

use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize)]
pub struct ContractDefinition {
    contract_id: String, // TODO: make it a hash
    contract_name: String, // TODO: limit to 32 chars
    contract_issuer: String, // TODO: make it a pubkey
    contract_spec: ContractSpecification,
}

#[derive(Serialize, Deserialize)]
pub struct ContractSpecification {
    runtime: String,
    public_functions: Vec<PublicFunction>,
    initialization: Vec<FunctionCall>
}

#[derive(Serialize, Deserialize)]
pub struct PublicFunction {
    name: String, // TODO: limit it to 32 chars
    function: FunctionRef,
    argument_def: HashMap<String, ArgType>
}

#[derive(Serialize, Deserialize)]
pub struct FunctionCall {
    function: FunctionRef,
    arguments: HashMap<String, ArgType>
}

#[derive(Serialize, Deserialize)]
pub struct FunctionRef {
    template_func: String, // TODO: limit to 32 chars
    template_id: String, // TODO: make it a hash
}

#[derive(Serialize, Deserialize)]
pub enum ArgType {
    String,
    UInt64
}