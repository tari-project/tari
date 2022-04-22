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
import baseNodeI18n from './baseNode'

const translations = {
  common: commonI18n,
  mining: miningI18n,
  baseNode: baseNodeI18n,
}

export default translations
