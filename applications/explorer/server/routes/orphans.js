var express = require('express');
var pool = require('../db');
var router = express.Router();

router.get('/', function(req, res, next) {

        pool.query(`SELECT hash, header
                    FROM orphan_blocks
                    ORDER BY created_at DESC`,
            (q_err, q_res) => {
                console.log(q_res.rows);
                res.json(q_res.rows)
            })
});


module.exports = router;
