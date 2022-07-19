import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { bytesToHex } from '../../utils/Format'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'
import { getIdentity } from './baseNodeService'
import { BaseNodeIdentity } from './types'

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

export const getBaseNodeIdentity = createAsyncThunk<
  BaseNodeIdentity,
  void,
  { state: RootState }
>('baseNode/getBaseNodeIdentity', async (_, thunkApi) => {
  try {
    const result = await getIdentity()
    return {
      publicAddress: result.publicAddress,
      publicKey: bytesToHex(result.publicKey),
      nodeId: bytesToHex(result.nodeId),
      emojiId: result.emojiId,
    }
  } catch (err) {
    return thunkApi.rejectWithValue(err)
  }
})
