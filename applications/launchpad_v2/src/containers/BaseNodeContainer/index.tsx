import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'

import { Select } from '../../components/Select'

const networks = ['mainnet', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

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
      <Select
        value={tariNetwork}
        options={networkOptions}
        onChange={setTariNetwork}
        label="Tari network"
        fullWidth
      />
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
