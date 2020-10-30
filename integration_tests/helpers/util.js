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

    while (new Date() - now < maxTime)  {
        const value = await asyncTestFn();
        if (value === toBe ) break;
        await sleep(100);
    }
}

function dec2hex (n){
    return n ? [n%256].concat(dec2hex(~~(n/256))) : [];
}

function toLittleEndianInner(n){

    let hexar = dec2hex(n);
    return hexar.map(h => (h < 16 ? "0" : "") + h.toString(16))
        .concat(Array(4-hexar.length).fill("00"));
}

function toLittleEndian(n, numBits) {

    let s = toLittleEndianInner(n);

    for (let i=s.length;i<numBits/8;i++) {
        s.push("00");
    }

    let arr = Buffer.from(s.join(''), 'hex');

    return arr;
}

module.exports = {
    getRandomInt,
    sleep,
    waitFor,
    toLittleEndian
};
