var net = require('net');

var {blake2bInit, blake2bUpdate, blake2bFinal} = require('blakejs');

function getRandomInt(min, max) {
    min = Math.ceil(min);
    max = Math.floor(max);
    return Math.floor(Math.random() * (max - min + 1)) + min;
}

function sleep(ms) {
    return new Promise((resolve) => {
        setTimeout(resolve, ms);
    });
}

async function waitFor(asyncTestFn, toBe, maxTime) {
    var now = new Date();

    while (new Date() - now < maxTime) {
        const value = await asyncTestFn();
        if (value === toBe) break;
        await sleep(500);
    }
}

function dec2hex(n) {
    return n ? [n % 256].concat(dec2hex(~~(n / 256))) : [];
}

function toLittleEndianInner(n) {

    let hexar = dec2hex(n);
    return hexar.map(h => (h < 16 ? "0" : "") + h.toString(16))
        .concat(Array(4 - hexar.length).fill("00"));
}

function toLittleEndian(n, numBits) {

    let s = toLittleEndianInner(n);

    for (let i = s.length; i < numBits / 8; i++) {
        s.push("00");
    }

    let arr = Buffer.from(s.join(''), 'hex');

    return arr;
}

function hexSwitchEndianness(val) {
    let res = "";
    for (let i=val.length - 2; i > 0; i -=2) {
        res += val[i] + val[i+1];
    }
    return res;
}

// Thanks to https://stackoverflow.com/questions/29860354/in-nodejs-how-do-i-check-if-a-port-is-listening-or-in-use
var portInUse = function (port, callback) {
    var server = net.createServer(function (socket) {
        socket.write('Echo server\r\n');
        socket.pipe(socket);
    });

    server.listen(port, '127.0.0.1');
    server.on('error', function (e) {
        callback(true);
    });
    server.on('listening', function (e) {
        server.close();
        callback(false);
    });
};

var index=0;
var getFreePort = async function (from, to) {
    function testPort(port) {
        return new Promise(r => {
            portInUse(port, (v) => {
                if (v) {
                    r(false);
                }
                r(true);
            });
        })
    }

    let port = from + index;
    if (port > to) {
       index = from;
       port = from;
    }
    while (port < to) {
        // let port = getRandomInt(from, to);
        // await sleep(100);
        port++;
        index++;
        let notInUse = await testPort(port);
        // console.log("Port not in use:", notInUse);
        if (notInUse) {
            return port
        }
    }
}

// WIP  this doesn't hash properly
const getTransactionOutputHash = function(output) {
    var KEY = null // optional key
    var OUTPUT_LENGTH = 32 // bytes
    var context = blake2bInit(OUTPUT_LENGTH, KEY);
    let flags = Buffer.alloc(1);
    flags[0] =output.features.flags;
    var buffer = Buffer.concat([flags, toLittleEndian(parseInt(output.features.maturity),64)]);
    blake2bUpdate(context,buffer);
    blake2bUpdate(context,output.commitment);
    let final = blake2bFinal(context);
    return Buffer.from(final);
}

module.exports = {
    getRandomInt,
    sleep,
    waitFor,
    toLittleEndian,
    // portInUse,
    getFreePort,
    getTransactionOutputHash,
    hexSwitchEndianness
};
