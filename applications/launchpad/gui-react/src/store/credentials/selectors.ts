import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '../'

export const selectWallet = ({ credentials }: RootState) =>
  credentials.wallet || ''
export const selectMoneroUsername = ({ credentials }: RootState) =>
  credentials.monero?.username || ''
export const selectMoneroPassword = ({ credentials }: RootState) =>
  credentials.monero?.password || ''

export const selectIsParoleSet = createSelector(selectWallet, wallet =>
  Boolean(wallet),
)

export const selectAreMoneroCredentialsPresent = createSelector(
  selectMoneroUsername,
  selectMoneroPassword,
  (username, password) => {
    return Boolean(username && password)
  },
)
