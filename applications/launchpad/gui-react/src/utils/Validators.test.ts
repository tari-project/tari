import { isUrl } from './Validators'

describe('Validators', () => {
  it('isUrl', () => {
    // invalid cases:
    expect(isUrl('aaa')).toBe(false)
    expect(isUrl('10.25.868.974.41')).toBe(false)
    expect(isUrl('///')).toBe(false)
    expect(isUrl('http//example.com')).toBe(false)

    // Protocol is required:
    expect(isUrl('aaa.com')).toBe(false)
    expect(isUrl('aaa.bbb.com')).toBe(false)
    expect(isUrl('http://aaa.bbb.com')).toBe(true)
    expect(isUrl('http://example.com')).toBe(true)
    expect(isUrl('http://example.com:1234/sub?q=3')).toBe(true)
    expect(isUrl('https://example.com:1234/sub?q=3')).toBe(true)
    expect(isUrl('ftp://example.com:1234/sub?q=3')).toBe(true)
    expect(isUrl('ws://example.com:1234/sub?q=3')).toBe(true)
    expect(isUrl('anotherprotocol://example.com:1234/sub?q=3')).toBe(true)

    // IP address is valid:
    expect(isUrl('http://10.0.0.1:1234/sub?q=3')).toBe(true)

    // Port address is supported:
    expect(isUrl('http://10.0.0.1:1234/sub?q=3')).toBe(true)
    expect(isUrl('http://example.some/sub?q=3')).toBe(true)

    // subdomains are supported:
    expect(isUrl('http://sub.example.some/sub?q=3')).toBe(true)
    expect(isUrl('http://sub1.sub2.example.some/sub?q=3')).toBe(true)
    expect(isUrl('http://www.example.some/sub?q=3')).toBe(true)

    // subpath and query strings are valid:
    expect(isUrl('http://example.some/sub/sub2?q=3')).toBe(true)
    expect(isUrl('http://example.some?q=3')).toBe(true)
    expect(isUrl('http://example.some?q=3&x=12&ysome-string%20C')).toBe(true)
  })
})
