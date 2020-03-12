const {Pool} = require('pg')

const pool = new Pool({
    user: 'postgres',
    host: 'localhost',
    database: 'tari_base_node',
    password: 'password123',
    post: 5432
})

module.exports = pool