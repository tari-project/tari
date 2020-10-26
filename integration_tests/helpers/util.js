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

module.exports = {
    getRandomInt,
    sleep,
    waitFor
};
