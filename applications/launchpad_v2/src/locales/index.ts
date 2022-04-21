/**
 * I18n
 * 
 * @example
 * import t from './locales'
 * 
 * <p>{t.common.nouns.wallet}</p>
 */
import commonI18n from './common'

import footerI18n from './footer'
import miningI18n from './mining'

const transalations = {
  common: commonI18n,
  footer: footerI18n,
  mining: miningI18n,
}

export default transalations