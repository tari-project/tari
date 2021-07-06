// Copyright 2021. The Tari Project
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

use crate::dan_layer::TokenId;
use patricia_tree::PatriciaMap;
use patricia_tree::node::{NodeEncoder, NodeDecoder, Node};
use bytecodec::null::{NullEncoder, NullDecoder};
use bytecodec::Encode;
use bytecodec::io::{IoEncodeExt, IoDecodeExt};
use std::path::PathBuf;
use crate::digital_assets_error::DigitalAssetError;
use std::fs::File;
use bytecodec::bytes::{BytesEncoder, BytesDecoder, CopyableBytesDecoder};
use serde_json::Value;
use bytecodec::json_codec::{JsonDecoder, JsonEncoder};

pub trait AssetDataStore {
    fn replace_metadata(&mut self, token_id:&TokenId, metadata: Vec<u8>) -> Result<(), DigitalAssetError>;
}

pub struct FileAssetDataStore {
   token_metadata: PatriciaMap<Value>,
   // None if dirty and must be regenerated
   merkle_root: Option<Vec<u8>>,
    output_file: PathBuf

}

impl FileAssetDataStore {

    pub fn load_or_create(output_file: PathBuf) -> Self {
        dbg!(&output_file);
        let token_metadata = match File::open(output_file.as_path()) {
            Ok(f) => {
                let mut decoder = NodeDecoder::new(JsonDecoder::new());
                let node = decoder.decode_exact(f).unwrap();
                let set = PatriciaMap::from(node);
                set
            },
            Err(_) => PatriciaMap::new()
        };

        Self {
            token_metadata,
            merkle_root: None,
            output_file
        }
    }

    pub fn save_to_file(&self) -> Result<(), DigitalAssetError> {
        let mut encoder = NodeEncoder::new(JsonEncoder::new());
        // TODO: sort unwraps
        encoder.start_encoding(self.token_metadata.clone().into()).unwrap();
        let mut output_file = File::create(self.output_file.as_path()).unwrap();
        encoder.encode_all(output_file).unwrap();
        Ok(())
    }
}

impl AssetDataStore for FileAssetDataStore {
    fn replace_metadata(&mut self, token_id: &TokenId, metadata: Vec<u8>) -> Result<(), DigitalAssetError> {
        let json = String::from_utf8(metadata).unwrap();
        dbg!(&json);
        let value = serde_json::from_str(&json).unwrap();
        self.token_metadata.insert(token_id, value);
        self.save_to_file()
    }
}

#[cfg(test)]
mod test{
    use crate::dan_layer::asset_data_store::{FileAssetDataStore, AssetDataStore};
    use std::path::PathBuf;
    use std::env::consts::OS;
    use tari_test_utils::paths::{temp_tari_path, create_temporary_data_path};
    use crate::dan_layer::TokenId;

    #[test]
    fn test_create() {
        let temp_path = create_temporary_data_path().join("file-asset-data-store");
        {
            let mut store = FileAssetDataStore::load_or_create(temp_path.clone());
            store.replace_metadata(&TokenId(vec![11u8]), Vec::from("[1,2,3]".as_bytes())).unwrap();
        }
        let store2 = FileAssetDataStore::load_or_create(temp_path);
        dbg!(store2.token_metadata);

    }

}
