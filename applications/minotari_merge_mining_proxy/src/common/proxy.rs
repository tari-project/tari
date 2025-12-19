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

//! Helper functions for converting [responses](Response).

use std::convert::TryInto;

use bytes::{BufMut, BytesMut};
use http_body::Body;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{header, header::HeaderValue, http::response, Response, StatusCode, Version};
use serde_json as json;

use crate::{error::MmProxyError, proxy::service::ProxyBody};

pub async fn convert_json_to_hyper_json_response(
    resp: json::Value,
    code: StatusCode,
) -> Result<Response<json::Value>, MmProxyError> {
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.append("Content-Type", HeaderValue::from_str("application/json").unwrap());

    builder = builder.version(Version::HTTP_11).status(code);

    let body = resp;
    let resp = builder.body(body)?;
    Ok(resp)
}

/// Build body response from json.
///
/// # Errors
///
/// Return error when body is invalid.
pub fn json_response(status: StatusCode, body: &json::Value) -> Result<Response<ProxyBody>, MmProxyError> {
    let (body, len) = encode_json_body(body)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json".to_string())
        .header(header::CONTENT_LENGTH, len)
        .status(status)
        .body(body)
        .map_err(Into::into)
}

fn encode_json_body(body: &json::Value) -> Result<(ProxyBody, usize), MmProxyError> {
    let bytes = BytesMut::new();
    let mut writer = bytes.writer();
    json::to_writer(&mut writer, body)?;
    let bytes = writer.into_inner().freeze();
    let len = bytes.len();
    let body = BoxBody::new(Full::new(bytes));
    Ok((body, len))
}

/// Convert parts and content into body response.
pub fn into_response(mut parts: response::Parts, content: &json::Value) -> Result<Response<ProxyBody>, MmProxyError> {
    let (body, size) = encode_json_body(content)?;
    // Ensure that the content length header is correct
    // TODO: check if this is necessary
    parts.headers.insert(header::CONTENT_LENGTH, size.into());
    parts
        .headers
        .insert(header::CONTENT_TYPE, "application/json".try_into().unwrap());
    Ok(Response::from_parts(parts, body))
}

/// Convert json response to body response.
pub fn into_body_from_response(resp: Response<json::Value>) -> Result<Response<ProxyBody>, MmProxyError> {
    let (parts, body) = resp.into_parts();
    into_response(parts, &body)
}

/// Reads the body until there is no more to read.
pub async fn read_body_until_end<B>(body: B) -> Result<BytesMut, MmProxyError>
where
    B: Body + Unpin,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let collected = body
        .collect()
        .await
        .map_err(|e| MmProxyError::InvalidMonerodResponse(format!("Failed to read body until the end: {e}")))?;
    Ok(BytesMut::from(collected.to_bytes().as_ref()))
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[tokio::test]
    async fn test_convert_json_to_hyper_json_response() {
        let resp = json::json!({"test key":"test value"});
        let code = StatusCode::from_u16(200).unwrap();
        // println!("{:?}", resp);
        let hyper = convert_json_to_hyper_json_response(resp.clone(), code).await.unwrap();
        assert_eq!(hyper.status(), code);
        assert_eq!(hyper.body(), &resp);
        assert_eq!(hyper.version(), Version::HTTP_11);
    }

    #[tokio::test]
    async fn test_json_response_and_read_body_until_end() {
        let status = StatusCode::from_u16(200).unwrap();
        let body = json::json!({"test key":"test value"});
        let response = json_response(status, &body.clone()).unwrap();
        assert_eq!(response.status(), status);
        assert!(response.headers().contains_key("content-type"));
        assert_eq!(response.headers()["content-type"], "application/json");
        assert!(response.headers().contains_key("content-length"));
        assert_eq!(response.headers()["content-length"], body.to_string().len().to_string());
        let bytes = read_body_until_end(response.into_body()).await.unwrap();
        assert_eq!(bytes, serde_json::to_vec(&body).unwrap());
    }

    #[test]
    pub fn test_into_body_from_response() {
        let body = json::json!({"test key": "test value"});
        let resp = Response::new(body.clone());
        let response = into_body_from_response(resp).unwrap();
        assert!(response.headers().contains_key("content-type"));
        assert_eq!(response.headers()["content-type"], "application/json");
        assert!(response.headers().contains_key("content-length"));
        assert_eq!(response.headers()["content-length"], body.to_string().len().to_string());
    }
}
