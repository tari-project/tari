// Copyright 2019, The Tari Project
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

use std::path::PathBuf;

const PROTOS_PATH: &'static str = "src/proto";

fn walk_protos(search_path: &PathBuf) -> Vec<PathBuf> {
    let mut protos = Vec::new();
    let paths_iter = search_path
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .map(|dir| dir.path());

    for path in paths_iter {
        if path.is_file() && path.extension().filter(|ext| ext == &"proto").is_some() {
            protos.push(path)
        } else if path.is_dir() {
            protos.extend(walk_protos(&path));
        }
    }

    protos
}

fn main() {
    let proto_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROTOS_PATH);
    let protos = walk_protos(&proto_path);

    println!("Compiling {} protobuf file(s)", protos.len());
    prost_build::Config::new()
        .compile_protos(&protos, &[proto_path])
        .unwrap();
}
