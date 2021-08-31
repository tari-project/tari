//  Copyright 2021, The Tari Project
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

//  Copyright 2020, The Tari Project
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

use super::PeerSeedResolver;
use tari_utilities::hex::Hex;
use trust_dns_client::rr::{rdata, RData, Record, RecordType};

// Ignore as this test requires network IO
#[ignore]
#[tokio::test]
async fn it_returns_an_empty_vec_if_all_seeds_are_invalid() {
    let mut resolver = PeerSeedResolver::connect("1.1.1.1:53".parse().unwrap()).await.unwrap();
    let seeds = resolver.resolve("tari.com").await.unwrap();
    assert!(seeds.is_empty());
}

fn create_txt_record(contents: Vec<String>) -> Record {
    let mut record = Record::new();
    record
        .set_record_type(RecordType::TXT)
        .set_rdata(RData::TXT(rdata::TXT::new(contents)));
    record
}

#[allow(clippy::vec_init_then_push)]
#[tokio::test]
async fn it_returns_peer_seeds() {
    let mut records = Vec::new();
    // Multiple addresses(works)
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::/onion3/\
         bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234"
            .into(),
    ]));
    // Misc
    records.push(create_txt_record(vec!["v=spf1 include:_spf.spf.com ~all".into()]));
    // Single address (works)
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000".into(),
    ]));
    // Single address trailing delim
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::".into(),
    ]));
    // Invalid public key
    records.push(create_txt_record(vec![
        "07e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000".into(),
    ]));
    // No Address with delim
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::".into(),
    ]));
    // No Address no delim
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a".into(),
    ]));
    // Invalid address
    records.push(create_txt_record(vec![
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/invalid:1234".into(),
    ]));
    let mut resolver = PeerSeedResolver::connect_test(records).await.unwrap();
    let seeds = resolver.resolve("tari.com").await.unwrap();
    assert_eq!(seeds.len(), 2);
    assert_eq!(
        seeds[0].public_key.to_hex(),
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
    );
    assert_eq!(
        seeds[1].public_key.to_hex(),
        "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
    );
    assert_eq!(seeds[0].addresses.len(), 2);
    assert_eq!(seeds[1].addresses.len(), 1);
}
