import { createAsyncThunk } from '@reduxjs/toolkit'

import { AppDispatch } from '../'
import { loadDefaultServiceSettings } from '../settings/thunks'
import { actions as dockerImagesActions } from '../dockerImages'

export const init = createAsyncThunk<void, void, { dispatch: AppDispatch }>(
  'app/init',
  async (_, thunkApi) => {
    await thunkApi.dispatch(loadDefaultServiceSettings()).unwrap()
    await thunkApi.dispatch(dockerImagesActions.getDockerImageList()).unwrap()
  },
)
