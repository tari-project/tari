# Tari Launchpad

GUI to manage Tari Docker containers.

The Tari Launchpad is dedicated for beginners in Blockchain world, as well as for experienced users. The application helps the user to download Tari Docker images, run specific containers, and give the insight into the running containers.

## Getting started

**Minimal requirements**

* Rust (`> 1.58`)
* Node (`> 16.*`)
* Docker Engine & Docker Compose installed

**Techs**

* Tauri
* React
* Typescript
* Rust

**Steps**

```bash
# Installation
$ npm install

# Run the application
$ npm run tauri dev
```

### Other scripts

#### ESlint

```bash
$ npm run lint

# With auto-fix
$ npm run lint:fix
```

## Development notes

### Locales

The project doesn't support i18n, and doesn't use any i18n package. However, all texts are located in `./src/locales/*`. It's recommended to place any static text in the `./src/locales/*` and import into the component from there.

Recommendations:

1. Common words and phrases add to the `common.ts` file.
2. Use dedicated files for specific feature/view, ie. 'baseNode.ts` would contain texts from the Base Node view.
3. Avoid duplications

### GUI directory structure

- `assets`
- `components` - contains only basic UI elements, ie. buttons, cards, etc. The component should not be connected to the Redux store.
- `containers` - implements the logic and can be connected to the Redux.
- `layouts`
- `locales` - for now, we do not add any i18n package to manage this. Just use simple Context API
- `modules` - add things that could be worth to export to other projects
- `pages` - aka. routes. In our case, it will be probably just one page here.
- `store` - redux related code
- `styles` - Design system
- `types` - (?) not sure if we need this. It should contain common types used across the application. we keep it for now and remove at the end if not needed.
- `utils` - helpers etc.
