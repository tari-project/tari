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
import walletI18n from './wallet'
import expertViewI18n from './expertView'
import walletPasswordWizardI18n from './walletPasswordWizard'

const translations = {
  common: commonI18n,
  footer: footerI18n,
  mining: miningI18n,
  baseNode: baseNodeI18n,
  wallet: walletI18n,
  expertView: expertViewI18n,
  walletPasswordWizard: walletPasswordWizardI18n,
}

export default translations
