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

const assert = require("assert");
const { describe, it } = require("mocha");

describe("Automatic recovery", () => {
  let {
    RECOVERY_COMPLETE_REGEXP,
    RECOVERY_WORTH_REGEXP,
    FAILURE_REGEXP,
  } = require("./automatic_recovery_test");

  describe("Log monitoring", () => {
    const matchingExample =
      "Recovery complete! Scanned = 156637 in 846.87s (184.96 utxos/s), Recovered 261584 worth 65313128.904449 T";

    const nonMatchingExmaple =
      "Recovery process 100% complete (626188 of 626188 utxos).";

    const failureExample = "Attempt 1/10: Failed to complete wallet recovery";

    it("correctly recognises the success log entry", () => {
      let matching = RECOVERY_COMPLETE_REGEXP.exec(matchingExample);
      assert(matching, "expected match");
      assert.equal(
        +matching[1],
        156637,
        "expected num scanned to be extracted"
      );
    });

    it("ignores a non-matching entry", () => {
      let matching = RECOVERY_COMPLETE_REGEXP.exec(nonMatchingExmaple);
      assert(!matching, "expected no match");
    });

    it("extracts the value", () => {
      let matching = RECOVERY_WORTH_REGEXP.exec(matchingExample);
      assert(matching, "expected match");
      assert.equal(
        +matching[1],
        65313128.904449,
        "expected value to be extracted"
      );
    });

    it("matches a failure", () => {
      let matching = FAILURE_REGEXP.exec(failureExample);
      assert(matching, "expected match");
      assert.equal(+matching[1], 1, "expected value to be extracted");
      assert.equal(+matching[2], 10, "expected value to be extracted");
    });
  });
});
