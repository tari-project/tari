var {createClient} = require("../baseNodeClient");

var express = require('express');
var router = express.Router();

/* GET home page. */
router.get('/:height', async function(req, res, next) {
  let client = createClient();
  let height = req.params.height;

  let block = await client.getBlocks({heights:[height]});

  // console.log(block[0].block.header);
  // console.log(block[0].block.body);
  console.log(block[0].block.body.kernels[0]);
  res.render('blocks', { title: `Block at height:${block[0].block.header.height}`, height:height, prevHeight: parseInt(height) - 1, nextHeight: parseInt(height) + 1, block: block[0].block , pows: {"0": "Monero", "2": "SHA"}});
});

module.exports = router;
