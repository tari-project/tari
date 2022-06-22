// Generate int in range [b,e)
const range = (start, end) => Array.from({ length: end - start }, (_, i) => start + i);
const zip = (a, b) => a.map((k, i) => [k, b[i]]);

module.exports = {
  range,
  zip,
};
