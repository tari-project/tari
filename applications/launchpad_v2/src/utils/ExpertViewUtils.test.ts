import ExpertViewUtils from './ExpertViewUtils'

describe('ExpertViewUtils', () => {
  it('should properly convert expert drawer variant into value', () => {
    const defaultSize = '30%'

    const [openSize, invertedOpenSize] =
      ExpertViewUtils.convertExpertViewModeToValue('open', defaultSize)
    expect(openSize).toEqual(defaultSize)
    expect(invertedOpenSize).toEqual('70%')

    const [hiddenSize, invertedHiddenSize] =
      ExpertViewUtils.convertExpertViewModeToValue('hidden')
    expect(hiddenSize).toEqual('0%')
    expect(invertedHiddenSize).toEqual('100%')

    const [fullscreenSize, invertedFullscreenSize] =
      ExpertViewUtils.convertExpertViewModeToValue('fullscreen', defaultSize)
    expect(fullscreenSize).toEqual('100%')
    expect(invertedFullscreenSize).toEqual('0%')
  })
})
