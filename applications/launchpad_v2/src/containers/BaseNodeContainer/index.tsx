import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'

const networks = ['mainnet', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

/**
 * @TODO move user-facing text to i18n file when implementing
 */

const BaseNodeContainer = () => {
  const [images, setImages] = useState<string[]>([])
  const [tariNetwork, setTariNetwork] = useState(networkOptions[0])
  const theme = useTheme()

  useEffect(() => {
    const getFromBackend = async () => {
      const imagesFromBackend = await invoke<string[]>('image_list')
      setImages(imagesFromBackend)
    }

    getFromBackend()
  }, [])

  return (
    <>
      <Box
        border={false}
        gradient={{ start: theme.actionBackground, end: theme.accent }}
      >
        <Text>no border</Text>
      </Box>
      <Box>
        <Text type='header'>Base Node</Text>
        <div style={{ padding: '16px' }}>
          <Select
            value={tariNetwork}
            options={networkOptions}
            onChange={setTariNetwork}
            label='Tari network'
          />
        </div>

        <div style={{ backgroundColor: '#662FA1', padding: '16px' }}>
          <Select
            value={tariNetwork}
            options={networkOptions}
            onChange={setTariNetwork}
            label='Tari network'
            inverted
          />
        </div>

        <p>
          available docker images:
          <br />
          {images.map(img => (
            <em key={img}>
              {img}
              {', '}
            </em>
          ))}
        </p>
      </Box>
    </>
  )
}

export default BaseNodeContainer
