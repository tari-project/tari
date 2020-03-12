var express = require('express');
var pool = require('../db');
var router = express.Router();

router.get('/', function(req, res, next) {
    const hash = req.query.hash;
    if (hash) {
        pool.query(`SELECT *
                    FROM block_headers bh
                        inner join merkle_checkpoints mc
                            on bh.height = mc.rank 
                            and mc.mmr_tree = 'UTXO'
                    where hash ilike '%' || $1 || '%'
                   `, [hash],  (q_err, q_res) => {
            console.log(q_res.rows);
            res.json(q_res.rows[0])
        });
    }
    else {
        pool.query(`SELECT bh.*, array_length(mc.nodes_added,1) as utxos
                    FROM block_headers bh
                             inner join merkle_checkpoints mc
                                        on bh.height = mc.rank
                                            and mc.mmr_tree = 'UTXO' 
                    ORDER BY height DESC`,
            (q_err, q_res) => {
                console.log(q_res.rows);
                res.json(q_res.rows)
            })
    }
});


module.exports = router;
