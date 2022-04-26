import { createAsyncThunk } from '@reduxjs/toolkit'

export const startNode = createAsyncThunk('startNode', async (_, thunkAPI) => {
  const {
    baseNode: { network },
  } = thunkAPI.getState()

  console.log(`starting base node on network ${network}`)
  await new Promise(resolve => setTimeout(resolve, 2000))
})

export const stopNode = createAsyncThunk('stopNode', async (_, thunkAPI) => {
  const {
    baseNode: { network },
  } = thunkAPI.getState()

  console.log(`stopping base node on network ${network}`)
  await new Promise(resolve => setTimeout(resolve, 2000))
})
