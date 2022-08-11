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

use std::{fs, io, io::ErrorKind, path::Path, process::Command};

use cargo_toml::{Manifest, Product};

use super::module::WasmModule;

pub fn compile_template<P: AsRef<Path>>(package_dir: P, features: &[&str]) -> io::Result<WasmModule> {
    let mut args = ["build", "--target", "wasm32-unknown-unknown", "--release"]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if !features.is_empty() {
        args.push("--features".to_string());
        args.extend(features.iter().map(ToString::to_string));
    }

    let status = Command::new("cargo")
        .current_dir(package_dir.as_ref())
        .args(args)
        .status()?;
    if !status.success() {
        return Err(io::Error::new(
            ErrorKind::Other,
            format!("Failed to compile package: {:?}", package_dir.as_ref()),
        ));
    }

    // resolve wasm name
    let manifest = Manifest::from_path(&package_dir.as_ref().join("Cargo.toml")).unwrap();
    let wasm_name = if let Some(Product { name: Some(name), .. }) = manifest.lib {
        // lib name
        name
    } else if let Some(pkg) = manifest.package {
        // package name
        pkg.name.replace('-', "_")
    } else {
        // file name
        package_dir
            .as_ref()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
            .replace('-', "_")
    };

    // path of the wasm executable
    let mut path = package_dir.as_ref().to_path_buf();
    path.push("target");
    path.push("wasm32-unknown-unknown");
    path.push("release");
    path.push(wasm_name);
    path.set_extension("wasm");

    // return
    let code = fs::read(path)?;
    Ok(WasmModule::from_code(code))
}
