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

var { createClient } = require("../baseNodeClient");

var express = require("express");
var router = express.Router();

router.get("/:height", async function (req, res) {
  let client = createClient();
  let height = req.params.height;

  try {
    let block = await client.getBlocks({ heights: [height] });

    if (!block || block.length === 0) {
      res.status(404);
      res.render("404", { message: `Block at height ${height} not found` });
      return;
    }
    console.log(block);
    console.log(block[0].block.body.outputs[0]);
    res.render("blocks", {
      title: `Block at height:${block[0].block.header.height}`,
      height: height,
      prevHeight: parseInt(height) - 1,
      nextHeight: parseInt(height) + 1,
      block: block[0].block,
      pows: { 0: "Monero", 2: "SHA" },
    });
  } catch (error) {
    res.status(500);
    res.render("error", { error: error });
  }
});

module.exports = router;
