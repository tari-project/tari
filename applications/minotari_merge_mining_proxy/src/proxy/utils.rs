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
use bytes::{Bytes, BytesMut};
use hyper::{
    Request,
    Response,
    header::{HeaderName as HyperHeaderName, HeaderValue as HyperHeaderValue},
};
use minotari_app_grpc::tari_rpc;
use serde_json as json;
use serde_json::json;
use tari_utilities::hex::Hex;

use crate::{common::proxy::MAX_BODY_SIZE, error::MmProxyError};

/// The JSON object key name used for merge mining proxy response extensions
pub(crate) const MMPROXY_AUX_KEY_NAME: &str = "_aux";

/// Buffers a monerod response body, failing as soon as it exceeds [`MAX_BODY_SIZE`].
///
/// `Response::json()` reads the whole body first and is bounded only in time, by
/// `monerod_connection_timeout`, which at line rate is still hundreds of megabytes from a hostile or broken node -
/// and six of the default `monerod_url` entries are third-party hosts.
async fn read_monerod_body(resp: &mut reqwest::Response) -> Result<Bytes, MmProxyError> {
    // When the server declares an oversized body, reject it without reading any of it.
    let max_body_size = u64::try_from(MAX_BODY_SIZE).unwrap_or(u64::MAX);
    if resp.content_length().is_some_and(|length| length > max_body_size) {
        return Err(MmProxyError::BodyTooLarge(MAX_BODY_SIZE));
    }
    let mut body = BytesMut::new();
    loop {
        // `without_url` keeps the monerod URL (which may carry credentials or a query) out of the error
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| MmProxyError::MonerodRequestFailed(e.without_url()))?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MAX_BODY_SIZE {
            return Err(MmProxyError::BodyTooLarge(MAX_BODY_SIZE));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

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

    let body = read_monerod_body(&mut resp).await?;
    let body_json = json::from_slice(&body)
        .map_err(|e| MmProxyError::InvalidMonerodResponse(format!("response body is not valid JSON: {e}")))?;
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

/// Append chain data to the result object. If the result object is null, a JSON object is created.
///
/// ## Panics
///
/// If response["result"] is not a JSON object type or null.
pub fn append_aux_chain_data(mut response: json::Value, chain_data: json::Value) -> json::Value {
    let result = &mut response["result"];
    if result.is_null() {
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
mod test {
    use std::{convert::Infallible, net::SocketAddr};

    use futures::stream;
    use http_body_util::{Full, StreamBody, combinators::BoxBody};
    use hyper::{body::Frame, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use tokio::net::TcpListener;

    use super::*;

    /// Spawns a server on an ephemeral port that answers every request with `payload`.
    ///
    /// When `declare_length` is set the body reports its exact size, so hyper sends a `content-length` and the
    /// proxy can reject it before reading anything. Otherwise the body is streamed with chunked transfer encoding,
    /// where the only way to bound it is to stop reading.
    async fn spawn_body_server(payload: Bytes, declare_length: bool) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let payload = payload.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |_request| {
                        let payload = payload.clone();
                        async move {
                            let body: BoxBody<Bytes, Infallible> = if declare_length {
                                BoxBody::new(Full::new(payload))
                            } else {
                                BoxBody::new(StreamBody::new(stream::iter(vec![Ok::<_, Infallible>(Frame::data(
                                    payload,
                                ))])))
                            };
                            Ok::<_, Infallible>(Response::new(body))
                        }
                    });
                    let _result = http1::Builder::new().serve_connection(TokioIo::new(tcp), service).await;
                });
            }
        });
        addr
    }

    async fn convert_response_from(
        payload: Bytes,
        declare_length: bool,
    ) -> Result<Response<json::Value>, MmProxyError> {
        let addr = spawn_body_server(payload, declare_length).await;
        let resp = reqwest::get(format!("http://{addr}/get_height")).await.unwrap();
        convert_reqwest_response_to_hyper_json_response(resp).await
    }

    fn oversized_payload() -> Bytes {
        Bytes::from(vec![b'0'; MAX_BODY_SIZE.saturating_add(1)])
    }

    #[tokio::test]
    async fn an_oversized_monerod_response_is_rejected_before_it_is_read() {
        let err = convert_response_from(oversized_payload(), true).await.unwrap_err();
        assert!(matches!(err, MmProxyError::BodyTooLarge(MAX_BODY_SIZE)), "{err}");
    }

    #[tokio::test]
    async fn an_oversized_chunked_monerod_response_is_rejected_while_it_is_read() {
        let err = convert_response_from(oversized_payload(), false).await.unwrap_err();
        assert!(matches!(err, MmProxyError::BodyTooLarge(MAX_BODY_SIZE)), "{err}");
    }

    #[tokio::test]
    async fn a_monerod_response_within_the_cap_is_converted() {
        let response = convert_response_from(Bytes::from_static(b"{\"height\":42}"), true)
            .await
            .unwrap();
        assert_eq!(response.body()["height"].as_u64(), Some(42));
    }
}
