import { useEffect, useState } from 'react'
import { useSpring } from 'react-spring'
import { os } from '@tauri-apps/api'

import KeyboardKeys from '../KeyboardKeys'
import Text from '../Text'

import t from '../../locales'

import { FooterTextWrapper, StyledFooter } from './styles'

const TerminalInstructions = {
  linux: {
    text: t.footer.toOpenTerminal,
    keysImage: <KeyboardKeys keys={['Ctrl', 'T']} />,
  },
  darwin: {
    text: t.footer.toOpenTerminal,
    keysImage: <KeyboardKeys keys={['cmd', 'T']} />,
  },
  windows_nt: {
    text: t.footer.toOpenTerminal,
    keysImage: <KeyboardKeys keys={['Ctrl', 'T']} />,
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
        setOSType(
          detectedPlatform.toLowerCase() as 'linux' | 'windows_nt' | 'darwin',
        )
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
          ...textAnim,
        }}
      >
        {osType ? (
          <Text
            type='smallMedium'
            color='inherit'
            as={'span'}
            testId='terminal-instructions-in-footer'
          >
            {t.footer.press} {TerminalInstructions[osType]?.keysImage}{' '}
            {TerminalInstructions[osType]?.text}
          </Text>
        ) : null}
      </FooterTextWrapper>
    </StyledFooter>
  )
}

export default Footer
