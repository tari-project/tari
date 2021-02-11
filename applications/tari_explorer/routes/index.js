var {createClient} = require("../baseNodeClient");

var express = require('express');
var router = express.Router();

/* GET home page. */
router.get('/', async function(req, res, next) {
  let client = createClient();
  let from = parseInt(req.query.from || 0);
  let limit = parseInt(req.query.limit || '20');

  let tipInfo = await client.getTipInfo({});

  // Get one more header than requested so we can work out the difference in MMR_size
  let headers = await client.listHeaders({from_height: from, num_headers: limit + 1});
  for (var i=headers.length - 2; i>=0; i--) {
    headers[i].kernels = headers[i].kernel_mmr_size - headers[i + 1].kernel_mmr_size;
    headers[i].outputs = headers[i].output_mmr_size - headers[i + 1].output_mmr_size;
  }
  let lastHeader = headers[headers.length - 1];
  if (lastHeader.height === '0') {
    // If the block is the genesis block, then the MMR sizes are the values to use
    lastHeader.kernels = lastHeader.kernel_mmr_size;
    lastHeader.outputs = lastHeader.output_mmr_size;
  } else {
    // Otherwise remove the last one, as we don't want to show it
    headers.splice(headers.length - 1, 1);
  }
  // console.log(headers);
  let firstHeight = parseInt(headers[0].height || "0");


  // --  mempool
  let mempool = await client.getMempoolTransactions({});

  for(let i=0; i< mempool.length; i++) {
    let sum = 0;
    for (let j=0;j<mempool[i].transaction.body.kernels.length;j++) {
      sum +=  parseInt(mempool[i].transaction.body.kernels[j].fee);
    }
    mempool[i].transaction.body.total_fees = sum;
  }

  res.render('index', { title: 'Blocks', tipInfo: tipInfo, mempool: mempool, headers: headers , pows: {"0": "Monero", "2": "SHA"}, nextPage:firstHeight-limit, prevPage:firstHeight+limit, limit: limit, from:from});
});

module.exports = router;
