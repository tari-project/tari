import Database from 'tauri-plugin-sql-api'

export let db: Database
const getDb = async () => {
  if (!db) {
    db = await Database.load('sqlite:launchpad.db')
  }

  return db
}
// load immediately to avoid waiting with first query
getDb()

export default getDb
