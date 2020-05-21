// Copyright 2019. The Tari Project
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

pub fn median(mut list: Vec<u64>) -> Option<f64> {
    if list.is_empty() {
        return None;
    }
    list.sort();
    let mid_index = list.len() / 2;
    let median = if list.len() % 2 == 0 {
        (list[mid_index - 1] + list[mid_index]) as f64 / 2.0
    } else {
        list[mid_index] as f64
    };
    Some(median)
}

pub fn mean(list: Vec<u64>) -> Option<f64> {
    if list.is_empty() {
        return None;
    }
    let mut count = 0;
    let total = list.iter().inspect(|_| count += 1).sum::<u64>();
    Some(total as f64 / count as f64)
}
/// TODO Implement the function for grpc responsed
pub fn quantile(_list: Vec<u64>) -> Option<f64> {
    None
}

/// TODO Implement the function for grpc responsed
pub fn quartile(_list: Vec<u64>) -> Option<f64> {
    None
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn median() {
        let mut values = vec![1u64, 8u64, 3u64, 9u64];
        let median_value = super::median(values);
        assert_eq!(median_value, Some(5.5f64))
    }

    #[test]
    fn mean() {
        let values = vec![1u64, 8u64, 3u64, 9u64];
        let mean_value = super::mean(values);
        assert_eq!(mean_value, Some(5.25f64))
    }
}
