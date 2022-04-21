import { useEffect, useState } from 'react'
import { useSpring } from 'react-spring'
import { os } from '@tauri-apps/api'

import t from '../../locales'

import { FooterTextWrapper, StyledFooter } from './styles'
import KeyboardKeys from '../KeyboardKeys'

/**
 * @TODO switch p to the Text component after merge and color from theme
 */

const TerminalInstructions = {
  linux: {
    text: t.footer.toOpenTerminal,
    keysImage: <KeyboardKeys keys={['Ctrl', 'Alt', 'T']} />,
  },
  darwin: {
    text: t.footer.toOpenTerminal,
    keysImage: <KeyboardKeys keys={['cmd', 'T']} />,
  },
  windows_nt: {
    text: t.footer.toOpenCommandPrompt,
    keysImage: <KeyboardKeys keys={['win', 'R']} />,
  },
}

/**
 * Footer component.
 *
 * The component render instructions how to open terminal on the host machine.
 * It supports only 'linux', 'windows (win32)' and 'macos (darwin)'.
 * If any other platform detected, then the text is not displayed.
 */
const Footer = () => {
  // const theme = useContext(ThemeContext)

  const [osType, setOSType] = useState<
    'linux' | 'windows_nt' | 'darwin' | null | undefined
  >(undefined)

  const textAnim = useSpring({
    opacity: osType === undefined ? 0 : 1,
  })

  useEffect(() => {
    checkPlatform()
  }, [])

  const checkPlatform = async () => {
    try {
      const detectedPlatform = await os.type()

      if (
        ['linux', 'windows_nt', 'darwin'].includes(
          detectedPlatform.toLowerCase(),
        )
      ) {
        setOSType(detectedPlatform as 'linux' | 'windows_nt' | 'darwin')
        return
      }

      setOSType(null)
    } catch (_err) {
      setOSType(null)
    }
  }

  return (
    <StyledFooter data-testid='footer-cmp'>
      <FooterTextWrapper
        style={{
          color: '#837A8B',
          ...textAnim,
        }}
      >
        {osType ? (
          <p data-testid='terminal-instructions-in-footer'>
            {t.footer.press} {TerminalInstructions[osType]?.keysImage}{' '}
            {TerminalInstructions[osType]?.text}
          </p>
        ) : null}
      </FooterTextWrapper>
    </StyledFooter>
  )
}

export default Footer
