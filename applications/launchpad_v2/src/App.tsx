import React, {useEffect, useState} from 'react';
import {invoke} from '@tauri-apps/api/tauri';

import logo from './logo.svg';
import './App.css';

function App() {
  const [images, setImages] = useState<string[]>([]);
  useEffect(() => {
    const getFromBackend = async () => {
      const imagesFromBackend = await invoke<string[]>('image_list')
      setImages(imagesFromBackend)
    }

    getFromBackend()
  }, [])

  return (
    <div className="App">
      <header className="App-header">
        <img src={logo} className="App-logo" alt="logo" />
        <p>
          Edit <code>src/App.tsx</code> and save to reload.
        </p>
        <a
          className="App-link"
          href="https://reactjs.org"
          target="_blank"
          rel="noopener noreferrer"
        >
          Learn React
        </a>

        <p>available docker images:<br/>
          {images.map(img => <em key={img}>{img}{', '}</em>)}
        </p>
      </header>
    </div>
  );
}

export default App;
