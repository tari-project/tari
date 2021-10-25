# tauri

If you get a rust build error:

```
error: proc macro panicked
  --> applications/tari_collectibles/src-tauri/src/main.rs:26:10
   |
26 |     .run(tauri::generate_context!())
   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: message: The `distDir` configuration is set to `"../web-app/build"` but this path doesn't exist
```

navigate to ../web-app and run (`npm i` if necessary, and then) `npm run build` (or `npm start` while developing)
