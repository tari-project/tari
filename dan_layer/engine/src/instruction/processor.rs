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

use std::{collections::HashMap, sync::Arc};

use crate::{
    instruction::{error::InstructionError, Instruction, InstructionSet},
    package::{Package, PackageId},
    runtime::{Runtime, RuntimeInterface},
    traits::Invokable,
    wasm::{ExecutionResult, Process},
};

#[derive(Debug, Clone, Default)]
pub struct InstructionProcessor<TRuntimeInterface> {
    packages: HashMap<PackageId, Package>,
    runtime_interface: TRuntimeInterface,
}

impl<TRuntimeInterface> InstructionProcessor<TRuntimeInterface>
where TRuntimeInterface: RuntimeInterface + Clone + 'static
{
    pub fn new(runtime_interface: TRuntimeInterface) -> Self {
        Self {
            packages: HashMap::new(),
            runtime_interface,
        }
    }

    pub fn load(&mut self, package: Package) -> &mut Self {
        self.packages.insert(package.id(), package);
        self
    }

    pub fn execute(&self, instruction_set: InstructionSet) -> Result<Vec<ExecutionResult>, InstructionError> {
        let mut results = Vec::with_capacity(instruction_set.instructions.len());

        // TODO: implement engine
        let state = Runtime::new(Arc::new(self.runtime_interface.clone()));
        for instruction in instruction_set.instructions {
            match instruction {
                Instruction::CallFunction {
                    package_id,
                    template,
                    function,
                    args,
                } => {
                    let package = self
                        .packages
                        .get(&package_id)
                        .ok_or(InstructionError::PackageNotFound { package_id })?;
                    let module = package
                        .get_module_by_name(&template)
                        .ok_or(InstructionError::TemplateNameNotFound { name: template })?;

                    // TODO: implement intelligent instance caching
                    let process = Process::start(module.clone(), state.clone())?;
                    let result = process.invoke_by_name(&function, args)?;
                    results.push(result);
                },
                Instruction::CallMethod {
                    package_id,
                    component_id,
                    method,
                    args,
                } => {
                    let package = self
                        .packages
                        .get(&package_id)
                        .ok_or(InstructionError::PackageNotFound { package_id })?;
                    // TODO: load component, not module - component_id is currently hard-coded as the template name in
                    // tests
                    let module = package
                        .get_module_by_name(&component_id)
                        .ok_or(InstructionError::TemplateNameNotFound { name: component_id })?;

                    // TODO: implement intelligent instance caching
                    let process = Process::start(module.clone(), state.clone())?;
                    let result = process.invoke_by_name(&method, args)?;
                    results.push(result);
                },
            }
        }

        Ok(results)
    }
}
