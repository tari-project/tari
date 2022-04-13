/**
 * I18n
 * 
 * @example
 * import t from './locales'
 * 
 * <p>{t.common.nouns.wallet}</p>
 */
import commonI18n from './common'

import miningI18n from './mining'

const transalations = {
  common: commonI18n,
  mining: miningI18n,
}

export default transalations