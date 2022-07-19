import { RootState } from '../'

import { selectMoneroUrls } from './selectors'

describe('mining/selectors', () => {
  it('should monero urls as string', () => {
    // given
    const rootState = {
      mining: {
        tari: {},
        merged: {
          threads: 1,
          urls: [{ url: 'first-url' }, { url: 'second-url' }],
        },
      },
    } as unknown as RootState

    const expected = 'first-url,second-url'

    // when
    const selected = selectMoneroUrls(rootState)

    // then
    expect(selected).toBe(expected)
  })
})
