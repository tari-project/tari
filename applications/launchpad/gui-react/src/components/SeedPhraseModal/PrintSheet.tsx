import { useEffect, useState } from 'react'

import Text from '../Text'
import Button from '../Button'

import t from '../../locales'

import SvgPrinter from '../../styles/Icons/Printer'
import { PrintView, PrintPhrase } from './styles'
import { listen } from '@tauri-apps/api/event'

const PrintSheet = ({ phrase }: { phrase: string[] }) => {
  const [printView, setPrintView] = useState(false)

  useEffect(() => {
    if (printView) {
      window.print()

      listen('tauri://focus', () => {
        setPrintView(false)
      })
    }
  }, [printView])

  if (!printView) {
    return (
      <Button
        variant='button-in-text'
        leftIcon={<SvgPrinter />}
        onClick={() => setPrintView(true)}
      >
        {t.settings.security.printRecoverySheet}
      </Button>
    )
  }

  return (
    <PrintView>
      <Text as='h2' type='subheader'>
        {t.settings.security.backupRecoveryPhrase}
      </Text>
      <PrintPhrase>
        <ol style={{ margin: 0 }}>
          {phrase.map((p, idx) => (
            <li key={idx}>
              <Text type='smallMedium'>{p}</Text>
            </li>
          ))}
        </ol>
      </PrintPhrase>
    </PrintView>
  )
}

export default PrintSheet
