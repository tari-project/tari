//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_template_types::models::{ComponentId, Contract, ContractAddress, Package, PackageId};

use crate::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub struct TemplateDef {
    pub template_name: String,
    pub functions: Vec<FunctionDef>,
}

impl TemplateDef {
    pub fn get_function(&self, name: &str) -> Option<&FunctionDef> {
        self.functions.iter().find(|f| f.name.as_str() == name)
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct FunctionDef {
    pub name: String,
    pub arguments: Vec<Type>,
    pub output: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum Type {
    Unit,
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    String,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CallInfo {
    pub func_name: String,
    pub args: Vec<Vec<u8>>,
    pub package: Package,
    pub contract: Contract,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct EmitLogArg {
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CreateComponentArg {
    pub contract_address: ContractAddress,
    pub module_name: String,
    pub package_id: PackageId,
    pub state: Vec<u8>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct GetComponentArg {
    pub component_id: ComponentId,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct SetComponentStateArg {
    pub component_id: ComponentId,
    pub state: Vec<u8>,
}
