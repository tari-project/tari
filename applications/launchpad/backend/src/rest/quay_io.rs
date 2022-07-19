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
//

use log::debug;
use serde::{Deserialize, Serialize};

use super::{list_image, DockerImageError};
use crate::docker::{ImageType, TariWorkspace, DOCKER_INSTANCE};

pub const QUAY_IO_REPO_NAME: &str = "quay.io";
pub const QUAY_IO_URL: &str = "https://quay.io/api/v1/repository";
pub const TARILABS_REPO_NAME: &str = "tarilabs";

#[derive(Serialize, Debug, Clone, Deserialize)]
struct QuayImageTag {
    name: String,
    reversion: bool,
    start_ts: u64,
    end_ts: Option<u64>,
    manifest_digest: String,
    is_manifest_list: bool,
    size: Option<u64>,
    last_modified: String,
    expiration: Option<String>,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
struct QuayTags {
    tags: Vec<QuayImageTag>,
    has_additional: bool,
    page: i32,
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct TagInfo {
    pub latest: bool,
    pub created_on: String,
    pub digest: String,
}

impl From<QuayImageTag> for TagInfo {
    fn from(source: QuayImageTag) -> Self {
        TagInfo {
            latest: true,
            created_on: source.last_modified,
            digest: source.manifest_digest,
        }
    }
}

fn quay_url(image_name: String, page: Option<i32>) -> String {
    match page {
        Some(p) => format!("{}/{}/tag/?page={}", QUAY_IO_URL, image_name, p),
        None => format!("{}/{}/tag/", QUAY_IO_URL, image_name),
    }
}

async fn get_image_tags(image: ImageType) -> Result<QuayTags, String> {
    if let ImageType::Loki | ImageType::Promtail | ImageType::Grafana = image {
        return Err(format!("image {} is not hosted on quay.io", image));
    }

    let quay_io_url = quay_url(format!("{}/{}", TARILABS_REPO_NAME, image.image_name()), None);
    let tag = reqwest::get(&quay_io_url)
        .await
        .map_err(|_| format!("Can't connect to: {}", &quay_io_url))?
        .json::<QuayTags>()
        .await
        .map_err(|_| format!("Can't read data from: {}", &quay_io_url))?;
    Ok(tag)
}

pub async fn get_tag_info(image: ImageType) -> Result<TagInfo, String> {
    let image_tag = get_image_tags(image).await?;
    let tags = image_tag.tags;
    let mut filtered: Vec<QuayImageTag> = tags
        .iter()
        .filter(|t| t.name.contains("latest") && t.expiration.is_none())
        .cloned()
        .collect();
    if filtered.is_empty() {
        Err("No tags found for tag [latest]".to_string())
    } else {
        if filtered.len() > 1 {
            filtered.sort_by(|t1, t2| t1.start_ts.cmp(&t2.start_ts));
        }
        Ok(TagInfo::from(filtered.pop().unwrap()))
    }
}

pub async fn is_up_to_date(image: ImageType, manifest_digest: String) -> Result<bool, DockerImageError> {
    let docker = DOCKER_INSTANCE.clone();
    let registry = format!("{}/{}", QUAY_IO_REPO_NAME, TARILABS_REPO_NAME);
    let fully_qualified_image_name = TariWorkspace::fully_qualified_image(image, Some(&registry));
    let image_ids: Vec<String> = list_image(fully_qualified_image_name.clone())
        .await?
        .iter()
        .map(|img| img.id.clone())
        .collect();

    for image_id in image_ids {
        let local_image = docker.inspect_image(&image_id).await?;
        let signagure = match local_image.repo_digests {
            Some(digests) => digests,
            None => return Ok(false),
        };
        debug!("signature: {:?}", signagure);
        if signagure.iter().any(|s| s.contains(&manifest_digest)) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tokio::test]
#[ignore]
async fn get_image_tags_test() {
    let image_tag = get_image_tags(ImageType::BaseNode).await.unwrap();
    assert!(!image_tag.tags.is_empty());
    println!("Found: {}", image_tag.tags.len());
    let image_tag = get_image_tags(ImageType::Wallet).await.unwrap();
    assert!(!image_tag.tags.is_empty());
    println!("Found: {}", image_tag.tags.len());
    let image_tag = get_image_tags(ImageType::Sha3Miner).await.unwrap();
    assert!(!image_tag.tags.is_empty());
    println!("Found: {}", image_tag.tags.len());
    let image_tag = get_image_tags(ImageType::Tor).await.unwrap();
    assert!(!image_tag.tags.is_empty());
    println!("Found: {}", image_tag.tags.len());
}

#[tokio::test]
#[ignore]
async fn is_up_to_date_test() {
    let tag_info = get_tag_info(ImageType::Tor).await.unwrap();
    println!("tor signature: {}", tag_info.digest);
    let up_to_date = is_up_to_date(ImageType::Tor, tag_info.digest).await.unwrap();
    assert!(up_to_date);
}

#[tokio::test]
#[ignore]
async fn get_tag_info_test() {
    let tag_info = get_tag_info(ImageType::Tor).await.unwrap();
    assert!(tag_info.latest);
    println!("found: {:?}", tag_info);
    let tag_info = get_tag_info(ImageType::BaseNode).await.unwrap();
    assert!(tag_info.latest);
    println!("found: {:?}", tag_info);
}
