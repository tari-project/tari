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

#![allow(clippy::indexing_slicing)]
use bytes::Bytes;
use hyper::{
    Request,
    Response,
    header::{HeaderName as HyperHeaderName, HeaderValue as HyperHeaderValue},
};
use minotari_app_grpc::tari_rpc;
use serde_json as json;
use serde_json::json;
use tari_utilities::hex::Hex;

use crate::error::MmProxyError;

/// The JSON object key name used for merge mining proxy response extensions
pub(crate) const MMPROXY_AUX_KEY_NAME: &str = "_aux";

/// The largest monerod response body the proxy will read. Monerod nodes are untrusted (A-MMPROXY-01); this is well
/// above the largest legitimate response (`get_block`, `get_transaction_pool`).
pub(crate) const MAX_MONEROD_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub async fn convert_reqwest_response_to_hyper_json_response(
    mut resp: reqwest::Response,
) -> Result<Response<json::Value>, MmProxyError> {
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    for (name, value) in resp.headers() {
        let hn = HyperHeaderName::from_bytes(name.as_str().as_bytes()).map_err(|e| {
            MmProxyError::ConversionError(format!("Failed to convert header name to hyper type: {}", e))
        })?;
        let hv = HyperHeaderValue::from_bytes(value.as_bytes()).map_err(|e| {
            MmProxyError::ConversionError(format!("Failed to convert header value to hyper type: {}", e))
        })?;
        headers.insert(hn, hv);
    }

    builder = builder.version(hyper::Version::HTTP_11).status(
        hyper::StatusCode::from_u16(resp.status().as_u16())
            .map_err(|e| MmProxyError::ConversionError(format!("Invalid status code: {}", e)))?,
    );

    let too_large = MmProxyError::MonerodResponseTooLarge {
        limit: MAX_MONEROD_RESPONSE_BYTES,
    };
    let content_length = resp
        .content_length()
        .map(|len| usize::try_from(len).unwrap_or(usize::MAX));
    if content_length.is_some_and(|len| len > MAX_MONEROD_RESPONSE_BYTES) {
        return Err(too_large);
    }
    let mut body = Vec::with_capacity(content_length.unwrap_or_default());
    // `without_url` keeps the monerod URL (which may carry credentials or a query) out of the error
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| MmProxyError::MonerodRequestFailed(e.without_url()))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_MONEROD_RESPONSE_BYTES {
            return Err(too_large);
        }
        body.extend_from_slice(&chunk);
    }
    let body_json = json::from_slice::<json::Value>(&body)?;
    let resp = builder.body(body_json)?;
    Ok(resp)
}

/// Add mmproxy extensions object to JSON RPC success response
pub fn add_aux_data(mut response: json::Value, mut ext: json::Value) -> json::Value {
    if response["result"].is_null() {
        return response;
    }
    match response["result"][MMPROXY_AUX_KEY_NAME].as_object_mut() {
        Some(obj_mut) => {
            let ext_mut = ext
                .as_object_mut()
                .expect("invalid parameter: expected `ext: json::Value` to be an object but it was not");
            obj_mut.append(ext_mut);
        },
        None => {
            response["result"][MMPROXY_AUX_KEY_NAME] = ext;
        },
    }
    response
}

/// Append chain data to the result object. If the result object is not a JSON object, a JSON object is created.
pub fn append_aux_chain_data(mut response: json::Value, chain_data: json::Value) -> json::Value {
    let result = &mut response["result"];
    if !result.is_object() {
        *result = json!({});
    }
    let chains = match result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut() {
        Some(arr_mut) => arr_mut,
        None => {
            result[MMPROXY_AUX_KEY_NAME]["chains"] = json!([]);
            result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut().unwrap()
        },
    };

    chains.push(chain_data);
    response
}

pub fn try_into_json_block_header(header: tari_rpc::BlockHeaderResponse) -> Result<json::Value, MmProxyError> {
    let tari_rpc::BlockHeaderResponse {
        header,
        reward,
        confirmations,
        difficulty,
        num_transactions,
    } = header;
    let header = header.ok_or_else(|| {
        MmProxyError::UnexpectedTariBaseNodeResponse(
            "Base node GRPC returned an empty header field when calling get_header_by_hash".into(),
        )
    })?;

    Ok(json!({
        "block_size": 0,
        "depth": confirmations,
        "difficulty": difficulty,
        "hash": header.hash.to_hex(),
        "height": header.height,
        "major_version": header.version,
        "minor_version": 0,
        "nonce": header.nonce,
        "num_txes": num_transactions,
        // Cannot be an orphan
        "orphan_status": false,
        "prev_hash": header.prev_hash.to_hex(),
        "reward": reward,
        "timestamp": header.timestamp
    }))
}

/// Convert a request with a Bytes body to a request with a json Value body
pub fn request_bytes_to_value(request: Request<Bytes>) -> Result<Request<json::Value>, MmProxyError> {
    let json = json::from_slice::<json::Value>(request.body())?;
    Ok(request.map(move |_| json))
}

#[cfg(test)]
pub(crate) mod test {
    use std::{convert::Infallible, net::SocketAddr, time::Duration};

    use futures::{StreamExt, stream};
    use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
    use hyper::{StatusCode, body::Frame, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use tokio::net::TcpListener;

    use super::*;

    pub(crate) type MockBody = BoxBody<Bytes, Infallible>;

    /// Serve `respond()` to every request on a loopback port, as a stand-in monerod.
    pub(crate) async fn serve_mock_monerod(respond: fn() -> Response<MockBody>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let svc = service_fn(move |_req| async move { Ok::<_, Infallible>(respond()) });
                    let _res = http1::Builder::new().serve_connection(TokioIo::new(tcp), svc).await;
                });
            }
        });
        addr
    }

    fn full(body: impl Into<Bytes>) -> MockBody {
        Full::new(body.into()).boxed()
    }

    /// Declares a Content-Length over the limit, sends a few bytes and then never finishes.
    pub(crate) fn oversized_content_length() -> Response<MockBody> {
        let frames = stream::once(async { Ok(Frame::data(Bytes::from_static(b"{\"a\""))) }).chain(stream::pending());
        Response::builder()
            .header(hyper::header::CONTENT_LENGTH, MAX_MONEROD_RESPONSE_BYTES + 1)
            .body(BodyExt::boxed(StreamBody::new(frames)))
            .unwrap()
    }

    async fn fetch(respond: fn() -> Response<MockBody>) -> Result<Response<json::Value>, MmProxyError> {
        let addr = serve_mock_monerod(respond).await;
        let resp = reqwest::get(format!("http://{addr}/json_rpc")).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(10),
            convert_reqwest_response_to_hyper_json_response(resp),
        )
        .await
        .expect("conversion should not hang")
    }

    #[tokio::test]
    async fn it_parses_a_small_body_and_keeps_status_and_headers() {
        let resp = fetch(|| {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("x-monerod", "yes")
                .body(full(r#"{"height": 5}"#))
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(resp.headers()["x-monerod"], "yes");
        assert_eq!(resp.body(), &json!({"height": 5}));
    }

    #[tokio::test]
    async fn it_rejects_an_oversized_content_length_without_reading_the_body() {
        let err = tokio::time::timeout(Duration::from_secs(1), fetch(oversized_content_length))
            .await
            .expect("should not wait for the body")
            .unwrap_err();
        assert!(matches!(err, MmProxyError::MonerodResponseTooLarge {
            limit: MAX_MONEROD_RESPONSE_BYTES
        }));
    }

    #[tokio::test]
    async fn it_rejects_an_oversized_chunked_body() {
        let err = fetch(|| {
            let chunk = Bytes::from(vec![b' '; 1024 * 1024]);
            let frames = stream::repeat_with(move || Ok(Frame::data(chunk.clone()))).take(9);
            Response::new(BodyExt::boxed(StreamBody::new(frames)))
        })
        .await
        .unwrap_err();
        assert!(matches!(err, MmProxyError::MonerodResponseTooLarge { .. }));
    }

    #[tokio::test]
    async fn it_accepts_a_body_exactly_at_the_limit() {
        let resp = fetch(|| {
            let body = format!("\"{}\"", "a".repeat(MAX_MONEROD_RESPONSE_BYTES - 2));
            Response::new(full(body))
        })
        .await
        .unwrap();
        assert_eq!(resp.body().as_str().unwrap().len(), MAX_MONEROD_RESPONSE_BYTES - 2);
    }

    #[tokio::test]
    async fn it_returns_a_json_error_for_invalid_json() {
        let err = fetch(|| Response::new(full("not json"))).await.unwrap_err();
        assert!(matches!(err, MmProxyError::JsonError(_)));
    }
}
