import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'

export const startNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/startNode',
  (_, thunkApi) =>
    thunkApi
      .dispatch(
        containersActions.startRecipe({ containerName: Container.BaseNode }),
      )
      .unwrap(),
)

export const stopNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/stopNode',
  (_, thunkApi) =>
    thunkApi
      .dispatch(containersActions.stopRecipe(Container.BaseNode))
      .unwrap(),
)
