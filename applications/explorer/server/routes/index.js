var express = require('express');
var router = express.Router();

router.get('/', function(req, res, next) {
  res.render('index', { version: 0.1 });
});

module.exports = router;
