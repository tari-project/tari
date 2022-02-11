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

use std::convert::TryInto;

use bytes::BytesMut;
use futures::StreamExt;
use hyper::{header, header::HeaderValue, http::response, Body, Response, StatusCode, Version};
use reqwest::{ResponseBuilderExt, Url};
use serde_json as json;

use crate::error::MmProxyError;

pub async fn convert_json_to_hyper_json_response(
    resp: json::Value,
    code: StatusCode,
    url: Url,
) -> Result<Response<json::Value>, MmProxyError> {
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.append("Content-Type", HeaderValue::from_str("application/json").unwrap());

    builder = builder.version(Version::HTTP_11).status(code).url(url);

    let body = resp;
    let resp = builder.body(body)?;
    Ok(resp)
}

pub fn json_response(status: StatusCode, body: &json::Value) -> Result<Response<Body>, MmProxyError> {
    let body_str = json::to_string(body)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json".to_string())
        .header(header::CONTENT_LENGTH, body_str.len())
        .status(status)
        .body(body_str.into())
        .map_err(Into::into)
}

pub fn into_response(mut parts: response::Parts, content: &json::Value) -> Response<Body> {
    let resp = json::to_string(content).expect("json::to_string cannot fail when stringifying a json::Value");
    // Ensure that the content length header is correct
    parts.headers.insert(header::CONTENT_LENGTH, resp.len().into());
    parts
        .headers
        .insert(header::CONTENT_TYPE, "application/json".try_into().unwrap());
    Response::from_parts(parts, resp.into())
}

pub fn into_body_from_response(resp: Response<json::Value>) -> Response<Body> {
    let (parts, body) = resp.into_parts();
    into_response(parts, &body)
}

/// Reads the `Body` until there is no more to read
pub async fn read_body_until_end(body: &mut Body) -> Result<BytesMut, MmProxyError> {
    let mut bytes = BytesMut::new();
    while let Some(data) = body.next().await {
        let data = data?;
        bytes.extend(data);
    }
    Ok(bytes)
}
