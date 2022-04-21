import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'

import Select from '../../components/Select'

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

  useEffect(() => {
    const getFromBackend = async () => {
      const imagesFromBackend = await invoke<string[]>('image_list')
      setImages(imagesFromBackend)
    }

    getFromBackend()
  }, [])

  return (
    <div>
      <h2>Base Node</h2>
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
    </div>
  )
}

export default BaseNodeContainer
