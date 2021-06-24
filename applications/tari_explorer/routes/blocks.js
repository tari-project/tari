var { createClient } = require('../baseNodeClient')

var express = require('express')
var router = express.Router()

/* GET home page. */
router.get('/:height', async function (req, res, next) {
  let client = createClient()
  let height = req.params.height

  try {
    let block = await client.getBlocks({ heights: [height] })

    if (!block || block.length === 0) {
      res.status(404);
      res.render('404', { message: `Block at height ${height} not found`});
      return;
    }
    console.log(block)
    console.log(block[0].block.body.outputs[0])
    res.render('blocks', {
      title: `Block at height:${block[0].block.header.height}`,
      height: height,
      prevHeight: parseInt(height) - 1,
      nextHeight: parseInt(height) + 1,
      block: block[0].block,
      pows: { '0': 'Monero', '2': 'SHA' }
    })

  } catch (error) {
    res.status(500)
    res.render('error', { error: error })
  }
})

module.exports = router
