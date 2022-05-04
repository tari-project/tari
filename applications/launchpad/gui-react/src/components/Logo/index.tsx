import SvgTariLaunchpadLogo from '../../styles/Icons/TariLaunchpadLogo'
import SvgTariLogo from '../../styles/Icons/TariLogo'
import SvgTariSignet from '../../styles/Icons/TariSignet'

import { LogoProps } from './types'

/**
 * Render given Logo variant.
 * @param {'logo' | 'signet' | 'full'} [variant = 'logo'] - selected variant
 *
 * - signet - only signet
 * - logo - signet with 'Tari'
 * - full - signet with 'Tari Launchpad'
 */
const Logo = ({ variant = 'logo' }: LogoProps) => {
  switch (variant) {
    case 'signet':
      return <SvgTariSignet />
    case 'full':
      return <SvgTariLaunchpadLogo />
    default:
      return <SvgTariLogo />
  }
}

export default Logo
