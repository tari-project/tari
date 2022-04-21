import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'

/**
 * @TODO move user-facing text to i18n file when implementing
 */

const BaseNodeContainer = () => {
  const [images, setImages] = useState<string[]>([])

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
      <div style={{padding: '16px'}}>
        <Select
          value={tariNetwork}
          options={networkOptions}
          onChange={setTariNetwork}
          label="Tari network"
        />
      </div>


      <div style={{backgroundColor: '#662FA1', padding: '16px'}}>
        <Select
          value={tariNetwork}
          options={networkOptions}
          onChange={setTariNetwork}
          label="Tari network"
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
