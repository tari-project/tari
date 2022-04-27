import { ReactNode } from 'react'

import { IconsWrapper, KeyTile, LetterKey } from './styles'
import { KeyboardKeysProps } from './types'

import SvgCmdKey from '../../styles/Icons/CmdKey'
import SvgWinKey from '../../styles/Icons/WinKey'

/**
 * Renders keyboard keys as set of tiles.
 * Use whenever you need to show the keyboard shortcuts, ie: "Ctrl + Alt + T"
 *
 * Use 'win' and 'cmd' to render Windows and Command keys.
 *
 * @param {string[]} keys - the set of keyboard keys
 *
 * @example
 * <KeyboardKeys keys={['Ctrl', 'Alt', 'win']} />
 */
const KeyboardKeys = ({ keys }: KeyboardKeysProps) => {
  const result: ReactNode[] = []

  keys.forEach((key, idx) => {
    let symbol
    switch (key) {
      case 'win':
        symbol = <SvgWinKey />
        break
      case 'cmd':
        symbol = <SvgCmdKey />
        break
      default:
        symbol = <LetterKey>{key}</LetterKey>
        break
    }

    result.push(<KeyTile key={`keyboard-key-${idx}`}>{symbol}</KeyTile>)
  })

  return <IconsWrapper>{result}</IconsWrapper>
}

export default KeyboardKeys
