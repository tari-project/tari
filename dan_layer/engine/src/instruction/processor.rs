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

use std::sync::Arc;

use tari_template_abi::encode;

use crate::{
    instruction::{error::InstructionError, Instruction, InstructionSet},
    packager::Package,
    runtime::{Runtime, RuntimeInterface},
    traits::Invokable,
    wasm::{ExecutionResult, Process},
};

#[derive(Debug, Clone)]
pub struct InstructionProcessor<TRuntimeInterface> {
    package: Package,
    runtime_interface: TRuntimeInterface,
}

impl<TRuntimeInterface> InstructionProcessor<TRuntimeInterface>
where TRuntimeInterface: RuntimeInterface + Clone + 'static
{
    pub fn new(runtime_interface: TRuntimeInterface, package: Package) -> Self {
        Self {
            package,
            runtime_interface,
        }
    }

    pub fn execute(&self, instruction_set: InstructionSet) -> Result<Vec<ExecutionResult>, InstructionError> {
        let mut results = Vec::with_capacity(instruction_set.instructions.len());

        // TODO: implement engine
        let state = Runtime::new(Arc::new(self.runtime_interface.clone()));
        for instruction in instruction_set.instructions {
            let result = match instruction {
                Instruction::CallFunction {
                    package_id,
                    template,
                    function,
                    args,
                } => {
                    if package_id != self.package.id() {
                        return Err(InstructionError::PackageNotFound { package_id });
                    }

                    let module = self
                        .package
                        .get_module_by_name(&template)
                        .ok_or(InstructionError::TemplateNameNotFound { name: template })?;

                    // TODO: implement intelligent instance caching
                    let process = Process::start(module.clone(), state.clone(), package_id)?;
                    process.invoke_by_name(&function, args)?
                },
                Instruction::CallMethod {
                    package_id,
                    component_id,
                    method,
                    args,
                } => {
                    if package_id != self.package.id() {
                        return Err(InstructionError::PackageNotFound { package_id });
                    }
                    let component = self.runtime_interface.get_component(&component_id)?;
                    let module = self.package.get_module_by_name(&component.module_name).ok_or_else(|| {
                        InstructionError::TemplateNameNotFound {
                            name: component.module_name.clone(),
                        }
                    })?;

                    let mut final_args = Vec::with_capacity(args.len() + 1);
                    final_args.push(encode(&component).unwrap());
                    final_args.extend(args);

                    // TODO: implement intelligent instance caching
                    let process = Process::start(module.clone(), state.clone(), package_id)?;
                    process.invoke_by_name(&method, final_args)?
                },
            };

            results.push(result);
        }

        Ok(results)
    }
}
