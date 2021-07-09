var { createClient } = require('../baseNodeClient')

var express = require('express')
var router = express.Router()

/* GET home page. */
router.get('/', async function (req, res, next) {
  try {
  let client = createClient()
  let from = parseInt(req.query.from || 0)
  let limit = parseInt(req.query.limit || '20')

  let tipInfo = await client.getTipInfo({})

    console.log("Getting headers");
  // Algo split
  let last100Headers = await client.listHeaders({ from_height: 0, num_headers: 101 })
  let monero = [0, 0, 0, 0]
  let sha = [0, 0, 0, 0]

    console.log(last100Headers);

  for (let i = 0; i < last100Headers.length - 1; i++) {
    let arr = last100Headers[i].pow.pow_algo === '0' ? monero : sha
    if (i < 10) {
      arr[0] += 1
    }
    if (i < 20) {
      arr[1] += 1
    }
    if (i < 50) {
      arr[2] += 1
    }
    arr[3] += 1

  }
  const algoSplit = {
    monero10: monero[0],
    monero20: monero[1],
    monero50: monero[2],
    monero100: monero[3],
    sha10: sha[0],
    sha20: sha[1],
    sha50: sha[2],
    sha100: sha[3]
  }

  console.log(algoSplit);
  // Get one more header than requested so we can work out the difference in MMR_size
  let headers = await client.listHeaders({ from_height: from, num_headers: limit + 1 })
  for (var i = headers.length - 2; i >= 0; i--) {
    headers[i].kernels = headers[i].kernel_mmr_size - headers[i + 1].kernel_mmr_size
    headers[i].outputs = headers[i].output_mmr_size - headers[i + 1].output_mmr_size
  }
  let lastHeader = headers[headers.length - 1]
  if (lastHeader.height === '0') {
    // If the block is the genesis block, then the MMR sizes are the values to use
    lastHeader.kernels = lastHeader.kernel_mmr_size
    lastHeader.outputs = lastHeader.output_mmr_size
  } else {
    // Otherwise remove the last one, as we don't want to show it
    headers.splice(headers.length - 1, 1)
  }

  // console.log(headers);
  let firstHeight = parseInt(headers[0].height || '0')

  // --  mempool
  let mempool = await client.getMempoolTransactions({})

  for (let i = 0; i < mempool.length; i++) {
    let sum = 0
    for (let j = 0; j < mempool[i].transaction.body.kernels.length; j++) {
      sum += parseInt(mempool[i].transaction.body.kernels[j].fee)
    }
    mempool[i].transaction.body.total_fees = sum
  }
  res.render('index', {
    title: 'Blocks',
    tipInfo: tipInfo,
    mempool: mempool,
    headers: headers,
    pows: { '0': 'Monero', '2': 'SHA' },
    nextPage: firstHeight - limit,
    prevPage: firstHeight + limit,
    limit: limit,
    from: from,
    algoSplit: algoSplit,
    blockTimes: getBlockTimes(last100Headers),
    moneroTimes: getBlockTimes(last100Headers, "0"),
    shaTimes: getBlockTimes(last100Headers, "1")
  })

} catch (error) {
  res.status(500)
  res.render('error', { error: error })
}
})


function getBlockTimes(last100Headers, algo) {
  let blocktimes = []
  let i = 0
  if (algo === '0' || algo === '1') {
    while (i < last100Headers.length && last100Headers[i].pow.pow_algo !== algo) {
      i++;
      blocktimes.push(0)
    }
  }
  if (i >= last100Headers.length) {
    // This happens if there are no blocks for a specific algorithm in last100headers
    return blocktimes;
  }
  let lastBlockTime = parseInt(last100Headers[i].timestamp.seconds);
  i++;
  while (i< last100Headers.length && blocktimes.length < 60) {
    if (!algo || last100Headers[i].pow.pow_algo === algo) {
      blocktimes.push((lastBlockTime - parseInt(last100Headers[i].timestamp.seconds)) / 60);
      lastBlockTime = parseInt(last100Headers[i].timestamp.seconds);
    }
    else{
      blocktimes.push(0)
    }
    i++;
  }
  return blocktimes
}

module.exports = router
