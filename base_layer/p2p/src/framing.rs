//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use futures::{AsyncRead, AsyncWrite};
use tokio_util::{
    codec::{Framed, LengthDelimitedCodec},
    compat::{Compat, FuturesAsyncReadCompatExt},
};

/// Canonical framing
pub type CanonicalFraming<T> = Framed<Compat<T>, LengthDelimitedCodec>;

/// Create a length-delimited frame around the given stream reader/writer with the given maximum frame length.
pub fn canonical<T>(stream: T, max_frame_len: usize) -> CanonicalFraming<T>
where T: AsyncRead + AsyncWrite + Unpin {
    Framed::new(
        stream.compat(),
        LengthDelimitedCodec::builder()
            .max_frame_length(max_frame_len)
            .new_codec(),
    )
}
