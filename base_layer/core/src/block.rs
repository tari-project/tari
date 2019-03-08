// Copyright 2018 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    blockheader::BlockHeader,
    transaction::{TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
};

/// A Tari block. Blocks are linked together into a blockchain.
pub struct Block {
    pub header: BlockHeader,
    pub body: AggregateBody,
}

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
pub struct AggregateBody {
    /// List of inputs spent by the transaction.
    pub inputs: Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    pub outputs: Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    pub kernels: Vec<TransactionKernel>,
}

impl AggregateBody {
    /// Create an empty aggregate body
    pub fn empty() -> AggregateBody {
        AggregateBody { inputs: vec![], outputs: vec![], kernels: vec![] }
    }

    /// Create a new aggregate body from provided inputs, outputs and kernels
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> AggregateBody
    {
        AggregateBody { inputs, outputs, kernels }
    }

    /// Add an input to the existing aggregate body
    pub fn add_input(mut self, input: TransactionInput) -> AggregateBody {
        self.inputs.push(input);
        self.inputs.sort();
        self
    }

    /// Add a series of inputs to the existing aggregate body
    pub fn add_inputs(mut self, mut inputs: Vec<TransactionInput>) -> AggregateBody {
        self.inputs.append(&mut inputs);
        self.inputs.sort();
        self
    }

    /// Add an output to the existing aggregate body
    pub fn add_output(mut self, output: TransactionOutput) -> AggregateBody {
        self.outputs.push(output);
        self.outputs.sort();
        self
    }

    /// Add an output to the existing aggregate body
    pub fn add_outputs(mut self, mut outputs: Vec<TransactionOutput>) -> AggregateBody {
        self.outputs.append(&mut outputs);
        self.outputs.sort();
        self
    }

    /// Add a kernel to the existing aggregate body
    pub fn add_kernel(mut self, kernel: TransactionKernel) -> AggregateBody {
        self.kernels.push(kernel);
        self.kernels.sort();
        self
    }

    /// Set the kernel of the aggregate body, replacing any previous kernels
    pub fn set_kernel(mut self, kernel: TransactionKernel) -> AggregateBody {
        self.kernels = vec![kernel];
        self
    }

    /// Sort the component lists of the aggregate body
    pub fn sort(&mut self) {
        self.inputs.sort();
        self.outputs.sort();
        self.kernels.sort();
    }

    /// Verify the signatures in all kernels contained in this aggregate body
    pub fn verify_kernel_signatures(&self) -> Result<(), TransactionError> {
        for kernel in self.kernels.iter() {
            kernel.verify_signature()?;
        }
        Ok(())
    }
}
